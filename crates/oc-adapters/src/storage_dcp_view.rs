//! VIS38 presentation uses the same atomic SQLite commit and ContextVersion
//! timeline as the existing projection. No transcript copy or replay/backfill.
use super::*;
use oc_core::dcp_view::{DcpAccounting, DcpRunSnapshot, DcpSummaryPage};
use std::collections::BTreeMap;

pub(super) const SNAPSHOT_CAP: usize = 16 * 1024;
pub const SUMMARY_PAGE_CAP: usize = 8192;
const INVALID_HOT: &str = "hot IS NOT NULL AND CASE WHEN json_valid(hot) THEN COALESCE(json_extract(hot,'$.version')!=1,1) OR COALESCE(json_type(hot,'$.active') NOT IN ('true','false'),1) OR COALESCE(json_type(hot,'$.standalone') NOT IN ('true','false'),1) ELSE 1 END";

pub(super) fn decode(raw: Option<String>) -> Option<DcpRunSnapshot> {
    raw.filter(|raw| raw.len() <= SNAPSHOT_CAP)
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

impl Db {
    pub(crate) fn renewal_read_counters(&self) -> [u64; 4] {
        std::array::from_fn(|i| {
            self.history_reads[i + 6].load(std::sync::atomic::Ordering::Relaxed)
        })
    }
    fn validate_hot_scope(conn: &Connection, session: &str) -> Result<(), StorageError> {
        let invalid:bool=conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM compression_blocks WHERE session_id=?1 AND {INVALID_HOT})"),[session],|r|r.get(0))?;
        if invalid {
            return Err(StorageError::CompressionConflict);
        }
        Ok(())
    }
    pub(crate) fn recent_completed_user_ids(
        &self,
        session: &str,
        count: u64,
    ) -> Result<Vec<String>, StorageError> {
        if count > 4096 {
            return Err(StorageError::CompressionConflict);
        }
        let conn = self.conn.lock().expect("db mutex");
        Ok(conn.prepare("SELECT m.id FROM conversation_messages m WHERE m.session_id=?1 AND m.role='user' AND (EXISTS(SELECT 1 FROM turn_acceptances a JOIN conversation_turns t ON t.id=a.turn_id WHERE a.user_message=m.id AND t.status='completed') OR EXISTS(SELECT 1 FROM conversation_messages z WHERE z.session_id=m.session_id AND z.role='assistant' AND z.seq>m.seq AND z.seq<COALESCE((SELECT MIN(seq) FROM conversation_messages n WHERE n.session_id=m.session_id AND n.role='user' AND n.seq>m.seq),9223372036854775807))) ORDER BY m.seq DESC LIMIT ?2")?.query_map(params![session,count as i64],|r|r.get(0))?.collect::<Result<Vec<_>,_>>()?)
    }
    /// Before a policy drops selected payload, capture only the causal identities
    /// of the admitted active wire. No call arguments, media or RAW pages transfer.
    pub(crate) fn wire_call_metadata(
        &self,
        session: &str,
        after: i64,
    ) -> Result<BTreeMap<DcpCallKey, DcpCallIdentity>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let owned:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM prefs WHERE key='dcp.projection_owned.'||?1 AND value='true')",[session],|r|r.get(0))?;
        let needed: bool = conn.query_row(
            if owned { "SELECT EXISTS(SELECT 1 FROM dcp_tool_projection_v2 WHERE session_id=?1 AND active=1)" } else { "SELECT EXISTS(SELECT 1 FROM dcp_tool_projection_v2 WHERE session_id=?1 AND (active=1 OR active IS NULL))" },
            [session],
            |r| r.get(0),
        )?;
        if !needed {
            return Ok(BTreeMap::new());
        }
        let sql="WITH roots AS (
          SELECT b.id,a.seq AS start,z.seq AS end,b.hot FROM compression_blocks b
          JOIN messages a ON a.id=b.start_msg JOIN conversation_messages z ON z.id=b.end_msg
          WHERE b.session_id=?1 AND z.seq>?2 AND b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 AND json_extract(b.hot,'$.standalone')=1
        ), logs AS (
          SELECT t.result AS log,m.seq AS position,m.seq AS source,t.status='started' AS pending FROM conversation_turns t
          JOIN conversation_messages m ON m.id=t.result->>'$.user_message'
          WHERE t.session_id=?1 AND json_valid(t.result) AND m.seq>?2
            AND NOT EXISTS(SELECT 1 FROM roots b WHERE m.seq BETWEEN b.start AND b.end)
          UNION ALL SELECT j.value,b.start,COALESCE((SELECT seq FROM messages WHERE id=j.value->>'$.user_message'),b.start),0
            FROM roots b,json_each(b.hot,'$.logs') j
          UNION ALL SELECT j.value,?2,COALESCE((SELECT seq FROM messages WHERE id=j.value->>'$.user_message'),?2),0
            FROM session_checkpoint c,json_each(c.selection) j WHERE c.session_id=?1
        ), inputs AS (
          SELECT log,position,source,0 AS part,0 AS pending,log->'$.working' AS items FROM logs WHERE log->'$.working' IS NOT NULL
          UNION ALL SELECT log,position,source,1,pending,log FROM logs
        ), calls AS (
          SELECT position,source,part,CAST(i.key AS INTEGER) AS idx,
            log->>'$.turn_id' AS turn,i.value->>'$.call_id' AS call,
            COALESCE((SELECT value FROM json_each(items,'$.call_occurrences') c WHERE CAST(c.key AS INTEGER)=i.key),
              (SELECT count(*) FROM json_each(items,'$.input') p WHERE p.key<i.key AND p.value->>'$.type'='function_call' AND p.value->>'$.call_id'=i.value->>'$.call_id')) AS original
          FROM inputs,json_each(CASE WHEN length(CAST(items AS BLOB))<=?3 THEN items END,'$.input') i
          WHERE i.value->>'$.type'='function_call' AND (pending=1 OR EXISTS(SELECT 1 FROM json_each(items,'$.input') o WHERE o.value->>'$.type'='function_call_output' AND o.value->>'$.call_id'=i.value->>'$.call_id'))
          UNION ALL SELECT position,source,part,-1,NULL,NULL,-1 FROM inputs WHERE length(CAST(items AS BLOB))>?3
        ), admitted AS (SELECT *,SUM(COALESCE(length(CAST(turn AS BLOB)),0)+COALESCE(length(CAST(call AS BLOB)),0)+32) OVER(ORDER BY position,source,part,idx) AS bytes FROM calls)
          SELECT CASE WHEN bytes<=?3 THEN turn END,CASE WHEN bytes<=?3 THEN call END,original FROM admitted ORDER BY position,source,part,idx LIMIT 4097";
        let mut result = BTreeMap::new();
        let mut counts = BTreeMap::<String, u64>::new();
        for row in conn.prepare(sql)?.query_map(
            params![
                session,
                after,
                crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )? {
            let (turn, id, original) = row?;
            self.count_history_read(8, turn.len() + id.len() + 8);
            if result.len() >= 4096 {
                return Err(StorageError::CompressionConflict);
            }
            let original =
                u64::try_from(original).map_err(|_| StorageError::CompressionConflict)?;
            let n = counts.entry(id.clone()).or_default();
            result.insert((id.clone(), *n), (turn, id, original));
            *n += 1;
        }
        Ok(result)
    }
    pub(super) fn active_intervals_in(
        conn: &Connection,
        session: &str,
        after: i64,
    ) -> Result<Vec<(i64, i64)>, StorageError> {
        Self::validate_hot_scope(conn, session)?;
        let rows=conn.prepare("SELECT a.seq,z.seq FROM compression_blocks b JOIN messages a ON a.id=b.start_msg JOIN conversation_messages z ON z.id=b.end_msg WHERE b.session_id=?1 AND b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 AND json_extract(b.hot,'$.standalone')=1 AND z.seq>?2 ORDER BY a.seq LIMIT 4097")?.query_map(params![session,after],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<Vec<_>,_>>()?;
        if rows.len() > 4096 {
            return Err(StorageError::CompressionConflict);
        }
        Ok(rows)
    }
    pub(super) fn renewal_schema(conn: &Connection) -> Result<(), StorageError> {
        let tx = conn.unchecked_transaction()?;
        for (table, column) in [
            ("compression_blocks", "hot"),
            ("session_checkpoint", "selection"),
            ("dcp_tool_projection_v2", "active"),
        ] {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            let columns = tx
                .prepare(&format!("PRAGMA table_info({table})"))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            if !columns.iter().any(|c| c == column) {
                let ty = if column == "active" {
                    "INTEGER CHECK(active IN (0,1) OR active IS NULL)"
                } else {
                    "TEXT"
                };
                tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ty};"))?;
            }
        }
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='compression_blocks')",[],|r|r.get(0))?;
        if exists {
            tx.execute_batch(&format!("CREATE INDEX IF NOT EXISTS compression_hot_invalid ON compression_blocks(session_id) WHERE {INVALID_HOT};"))?;
            tx.execute_batch("CREATE INDEX IF NOT EXISTS compression_hot_active ON compression_blocks(session_id,json_extract(hot,'$.active')) WHERE hot IS NOT NULL AND json_valid(hot); CREATE INDEX IF NOT EXISTS compression_legacy_scope ON compression_blocks(session_id,id) WHERE hot IS NULL; INSERT OR IGNORE INTO schema_migrations VALUES(9,'t45-hot-renewal');")?;
        }
        let projection:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='dcp_tool_projection_v2')",[],|r|r.get(0))?;
        if projection {
            tx.execute_batch("CREATE INDEX IF NOT EXISTS dcp_projection_active ON dcp_tool_projection_v2(session_id,active);")?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Public active log content only. A compressed turn contributes its real
    /// retained function pairs, not its superseded user/assistant archive.
    /// SQLite filters before transfer; one request has finite row/byte budgets.
    pub(crate) fn presentation_wire_logs(
        &self,
        session: &str,
        after_seq: i64,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
    ) -> Result<Option<Vec<String>>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let result = Self::presentation_wire_logs_in(&conn, session, after_seq, projected, blocks)?;
        if let Some(logs) = &result {
            for raw in logs {
                self.count_history_read(0, raw.len());
            }
        }
        Ok(result)
    }

    fn presentation_wire_logs_in(
        conn: &Connection,
        session: &str,
        after_seq: i64,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
    ) -> Result<Option<Vec<String>>, StorageError> {
        let ids = serde_json::to_string(&projected.iter().map(|r| &r.0).collect::<Vec<_>>())
            .map_err(|_| StorageError::CompressionConflict)?;
        let blocks = serde_json::to_string(
            &blocks
                .iter()
                .map(|b| serde_json::json!({"id": b.id, "members": b.members,"start":b.start_msg,"end":b.end_msg,"hot":b.hot}))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| StorageError::CompressionConflict)?;
        // Candidate membership overrides durable membership during pre-commit
        // measurement. Only public/retained fields cross SQLite's boundary;
        // covered canonical message/reasoning outputs are retained exactly as
        // in the Responses continuation, while ordinary Message input is not.
        let mut stmt = conn.prepare("WITH logs AS (
            SELECT t.result,t.archive_rowid,t.prompt,m.id,
              COALESCE((SELECT b.value->>'$.id' FROM json_each(?4) b,json_each(b.value,'$.members') member WHERE member.value=m.id LIMIT 1),
                (SELECT cm.block_id FROM compression_members cm WHERE cm.message_id=m.id AND cm.block_id IN (SELECT value->>'$.id' FROM json_each(?4)) LIMIT 1)) AS block_id
            FROM conversation_turns t JOIN conversation_messages m ON m.id=json_extract(t.result,'$.user_message')
            WHERE t.session_id=?1 AND json_valid(t.result) AND m.seq>?2
              AND (m.id IN(SELECT value FROM json_each(?3)) OR EXISTS(SELECT 1 FROM compression_members cm JOIN json_each(?4) b ON b.value->>'$.id'=cm.block_id WHERE cm.message_id=m.id AND COALESCE(b.value->>'$.hot.standalone',0)=0 AND COALESCE(b.value->>'$.hot.active',1)=1))
              AND NOT EXISTS(SELECT 1 FROM json_each(?4) b JOIN messages a ON a.id=b.value->>'$.start' JOIN messages z ON z.id=b.value->>'$.end' WHERE b.value->>'$.hot.standalone'=1 AND b.value->>'$.hot.active'=1 AND m.seq BETWEEN a.seq AND z.seq)
          ), visible AS (
            SELECT json_object('turn_id',result->>'$.turn_id','model',result->>'$.model','provider',result->>'$.provider',
               'agent_digest',result->>'$.agent_digest','user_message',id,'assistant_message',result->>'$.assistant_message',
               'raw_prefix',json(result->'$.raw_prefix'),'working',json(result->'$.working'),
                'requests',json(COALESCE(result->>'$.requests','[]')),
                'spans',json(COALESCE(result->>'$.spans','[]')),
               'call_occurrences',json(COALESCE((SELECT json_group_object(
                 (SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<CAST(c.key AS INTEGER)
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),c.value)
                  FROM json_each(logs.result,'$.call_occurrences') c WHERE EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') original WHERE original.key=CAST(c.key AS INTEGER) AND original.value->>'$.type'='function_call' AND EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=original.value->>'$.call_id'))),'{}')),
               'input_origins',json(COALESCE((SELECT json_group_array(i.key+COALESCE(result->>'$.raw_prefix.ends[0]',0)) FROM json_each(result,'$.input') i
                 WHERE (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                 AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),'[]')),
               'shell_notice_messages',json(CASE WHEN logs.id IN (SELECT value FROM json_each(?3)) THEN COALESCE(result->>'$.shell_notice_messages','[]') ELSE '[]' END),
               'instruction_references',json(COALESCE((SELECT json_group_array(json_object(
                 'event',a.value->>'$.event',
                 'index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.index'
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id')))))
                 FROM json_each(logs.result,'$.instruction_references') a),'[]')),
              '_dcp_block',CASE WHEN id IN (SELECT value FROM json_each(?3)) THEN NULL ELSE block_id END,
              '_dcp_prompt',CASE WHEN id IN (SELECT value FROM json_each(?3)) THEN prompt END,
              'display_parts',json(COALESCE((SELECT json_group_array(json(p.value)) FROM json_each(result,'$.display_parts') p WHERE p.value->>'$.tool' IS NOT NULL),'[]')),
              'input',json(COALESCE((SELECT json_group_array(json(i.value)) FROM json_each(result,'$.input') i
                WHERE (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                  AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),'[]')),
              'native_mcp_results',json(COALESCE((SELECT json_group_array(json_object(
                'input_index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.input_index'
                  AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                  AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),
                'call_id',a.value->>'$.call_id','result',json(a.value->'$.result')))
                 FROM json_each(logs.result,'$.native_mcp_results') a),'[]')),
               'native_read_results',json(COALESCE((SELECT json_group_array(json_object(
                 'input_index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.input_index'
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),
                 'call_id',a.value->>'$.call_id','result',json(a.value->'$.result')))
                 FROM json_each(logs.result,'$.native_read_results') a),'[]')))
              AS result,archive_rowid
            FROM logs WHERE id IN (SELECT value FROM json_each(?3)) OR block_id IS NOT NULL
          ) SELECT CASE WHEN length(CAST(result AS BLOB))<=?5 THEN result END FROM visible WHERE result->>'$._dcp_block' IS NULL OR json_array_length(result,'$.input')>0 ORDER BY archive_rowid LIMIT 4097")?;
        let mut logs = Vec::new();
        let mut budget = crate::runtime::ACTIVE_CONTEXT_BYTES_CAP;
        for row in stmt.query_map(
            params![session, after_seq, ids, blocks, budget as i64],
            |r| r.get::<_, Option<String>>(0),
        )? {
            let Some(raw) = row? else { return Ok(None) };
            if logs.len() >= 4096 || raw.len() > budget {
                return Ok(None);
            }
            budget -= raw.len();
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| StorageError::CompressionConflict)?;
            let turn = value["turn_id"]
                .as_str()
                .ok_or(StorageError::CompressionConflict)?;
            let prefix = crate::tools::turn_history::RawPrefix::parse(value.get("raw_prefix"))
                .map_err(|_| StorageError::CompressionConflict)?;
            Self::validate_raw_prefix(conn, turn, prefix.as_ref())?;
            logs.push(raw);
        }
        Ok(Some(logs))
    }
    /// Bounded active summary graph for presentation/measurement. Archive
    /// memberships and superseded unrelated blocks are never materialized.
    pub fn active_compression_graph(
        &self,
        session: &str,
        after_seq: i64,
    ) -> Result<Vec<crate::dcp::CompressionBlock>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::active_compression_graph_in(&conn, session, after_seq)
    }

    pub(super) fn active_compression_graph_in(
        conn: &Connection,
        session: &str,
        after_seq: i64,
    ) -> Result<Vec<crate::dcp::CompressionBlock>, StorageError> {
        let mut pending = Self::active_root_ids_in(conn, session, after_seq)?;
        Self::compression_graph_in(conn, session, &mut pending)
    }

    fn compression_graph_in(
        conn: &Connection,
        session: &str,
        pending: &mut Vec<String>,
    ) -> Result<Vec<crate::dcp::CompressionBlock>, StorageError> {
        Self::compression_graph_selected_in(conn, session, pending, true, None)
    }
    pub(crate) fn compaction_graph(
        &self,
        session: &str,
        after: i64,
        retain_selection: bool,
    ) -> Result<Vec<crate::dcp::CompressionBlock>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut roots = Self::active_root_ids_in(&conn, session, after)?;
        Self::compression_graph_selected_in(
            &conn,
            session,
            &mut roots,
            retain_selection,
            Some(self.history_reads.as_ref()),
        )
    }
    fn compression_graph_selected_in(
        conn: &Connection,
        session: &str,
        pending: &mut Vec<String>,
        retain_selection: bool,
        reads: Option<&[std::sync::atomic::AtomicU64; 10]>,
    ) -> Result<Vec<crate::dcp::CompressionBlock>, StorageError> {
        let mut graph = std::collections::BTreeMap::new();
        let mut budget = crate::runtime::ACTIVE_CONTEXT_BYTES_CAP;
        while let Some(id) = pending.pop() {
            if graph.contains_key(&id) {
                continue;
            }
            if graph.len() >= 4096 {
                return Err(StorageError::CompressionConflict);
            }
            let (topic, summary, start_msg, end_msg,raw_hot): (String,String,String,String,Option<String>) = conn.query_row("WITH chosen AS (SELECT topic,summary,start_msg,end_msg,CASE WHEN ?4 THEN hot ELSE json_set(hot,'$.logs',json('[]'),'$.protected',json('[]'),'$.legacy_protected',json('[]')) END AS hot FROM compression_blocks WHERE session_id=?1 AND id=?2) SELECT topic,summary,start_msg,end_msg,hot FROM chosen WHERE length(CAST(summary AS BLOB))+COALESCE(length(CAST(hot AS BLOB)),0)<=?3",params![session,id,budget as i64,retain_selection],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            if let Some(reads) = reads {
                use std::sync::atomic::Ordering::Relaxed;
                reads[6].fetch_add(1, Relaxed);
                reads[7].fetch_add(
                    (topic.len()
                        + summary.len()
                        + start_msg.len()
                        + end_msg.len()
                        + raw_hot.as_ref().map_or(0, String::len)) as u64,
                    Relaxed,
                );
            }
            let hot = raw_hot
                .map(|raw| {
                    serde_json::from_str::<serde_json::Value>(&raw)
                        .map_err(|_| StorageError::CompressionConflict)
                })
                .transpose()?;
            if hot.as_ref().is_some_and(|h| {
                h["version"] != 1 || !h["active"].is_boolean() || !h["standalone"].is_boolean()
            }) {
                return Err(StorageError::CompressionConflict);
            }
            budget = budget.saturating_sub(summary.len());
            budget = budget.saturating_sub(hot.as_ref().map_or(0, |h| h.to_string().len()));
            if !hot.as_ref().is_some_and(|h| h["standalone"] == true) {
                pending.extend(
                    crate::dcp::parse_block_placeholders(crate::dcp::authored_summary(&summary))
                        .into_iter()
                        .map(|p| p.block_id),
                );
            }
            graph.insert(
                id.clone(),
                crate::dcp::CompressionBlock {
                    hot,
                    id,
                    session: session.into(),
                    topic,
                    summary,
                    start_msg,
                    end_msg,
                    members: Vec::new(),
                },
            );
        }
        Ok(graph.into_values().collect())
    }

    pub(super) fn active_root_ids_in(
        conn: &Connection,
        session: &str,
        after: i64,
    ) -> Result<Vec<String>, StorageError> {
        Self::validate_hot_scope(conn, session)?;
        let sql = "SELECT cb.id FROM compression_blocks cb JOIN conversation_messages m ON m.id=cb.end_msg AND m.session_id=cb.session_id WHERE cb.session_id=?1 AND cb.hot IS NOT NULL AND json_valid(cb.hot) AND json_extract(cb.hot,'$.active')=1 AND m.seq>?2 UNION ALL SELECT cb.id FROM compression_blocks cb WHERE cb.session_id=?1 AND cb.hot IS NULL AND EXISTS(SELECT 1 FROM compression_members cm JOIN conversation_messages m ON m.id=cm.message_id WHERE cm.block_id=cb.id AND m.seq>?2) LIMIT 4097";
        let ids = conn
            .prepare(sql)?
            .query_map(params![session, after], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > 4096 {
            return Err(StorageError::CompressionConflict);
        }
        Ok(ids)
    }

    /// Bounded active/addressed planner input. Covered text is never reloaded:
    /// membership validation stays in SQL and only addressed stable IDs cross
    /// the boundary. Unrelated historical blocks/members are not Rust objects.
    #[allow(clippy::type_complexity)]
    pub(crate) fn compression_addressed_snapshot(
        &self,
        session: &str,
        after_seq: i64,
        ranges: &[crate::dcp::ValidatedRange],
    ) -> Result<
        (
            Vec<(String, String, String)>,
            Vec<crate::dcp::CompressionBlock>,
            Option<String>,
            u64,
            i64,
        ),
        StorageError,
    > {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut pending = Self::active_compression_graph_in(&conn, session, after_seq)?
            .into_iter()
            .map(|b| b.id)
            .collect::<Vec<_>>();
        let mut bounds = Vec::new();
        let mut referenced = std::collections::BTreeSet::new();
        for range in ranges {
            let mut endpoints = Vec::new();
            for (id, column) in [(&range.start_id, "start_msg"), (&range.end_id, "end_msg")] {
                let endpoint: Option<String> = conn
                    .query_row(
                        &format!(
                            "SELECT {column} FROM compression_blocks WHERE session_id=?1 AND id=?2"
                        ),
                        params![session, id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if endpoint.is_some() {
                    pending.push(id.clone());
                    referenced.insert(id.clone());
                }
                let seq: Option<i64> = conn
                    .query_row(
                        "SELECT seq FROM conversation_messages WHERE session_id=?1 AND id=?2",
                        params![session, endpoint.as_deref().unwrap_or(id)],
                        |r| r.get(0),
                    )
                    .optional()?;
                endpoints.push(seq);
            }
            if let (Some(start), Some(end)) = (endpoints[0], endpoints[1]) {
                bounds.push((start, end));
            }
            for p in crate::dcp::parse_block_placeholders(&range.summary) {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM compression_blocks WHERE session_id=?1 AND id=?2)",
                    params![session, p.block_id],
                    |r| r.get(0),
                )?;
                if exists {
                    pending.push(p.block_id.clone());
                    referenced.insert(p.block_id);
                }
            }
        }
        let bounds =
            serde_json::to_string(&bounds).map_err(|_| StorageError::CompressionConflict)?;
        for id in &referenced {
            let fully_covered: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM compression_blocks cb JOIN conversation_messages a ON a.id=cb.start_msg JOIN conversation_messages z ON z.id=cb.end_msg JOIN json_each(?2) span WHERE cb.id=?1 AND a.seq>=span.value->>'$[0]' AND z.seq<=span.value->>'$[1]')",params![id,bounds],|r|r.get(0))?;
            if !fully_covered {
                return Err(StorageError::CompressionConflict);
            }
        }
        let mut graph = Self::compression_graph_in(&conn, session, &mut pending)?;
        let mut endpoints = std::collections::BTreeSet::new();
        for block in &graph {
            endpoints.insert(block.start_msg.clone());
            endpoints.insert(block.end_msg.clone());
        }
        for range in ranges {
            for id in [&range.start_id, &range.end_id] {
                if !graph.iter().any(|b| &b.id == id) {
                    endpoints.insert(id.clone());
                }
            }
        }
        let endpoints =
            serde_json::to_string(&endpoints).map_err(|_| StorageError::CompressionConflict)?;
        let mut rows = Vec::new();
        let mut budget = crate::runtime::ACTIVE_CONTEXT_BYTES_CAP;
        let covered = "(EXISTS(SELECT 1 FROM compression_blocks cb JOIN messages a ON a.id=cb.start_msg JOIN messages z ON z.id=cb.end_msg WHERE cb.session_id=m.session_id AND cb.hot IS NOT NULL AND json_valid(cb.hot) AND json_extract(cb.hot,'$.active')=1 AND json_extract(cb.hot,'$.standalone')=1 AND m.seq BETWEEN a.seq AND z.seq) OR EXISTS(SELECT 1 FROM compression_members cm JOIN compression_blocks cb ON cb.id=cm.block_id WHERE cm.message_id=m.id AND (cb.hot IS NULL OR json_extract(cb.hot,'$.active')=1)))";
        let sql = format!(
            "WITH native AS (SELECT a.seq AS lo,z.seq AS hi FROM compression_blocks b JOIN messages a ON a.id=b.start_msg JOIN messages z ON z.id=b.end_msg WHERE b.session_id=?1 AND b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 AND json_extract(b.hot,'$.standalone')=1 AND z.seq>?2), gaps AS (SELECT ?2 AS lo,COALESCE((SELECT MIN(lo) FROM native),9223372036854775807) AS hi UNION ALL SELECT n.hi,COALESCE((SELECT MIN(lo) FROM native WHERE lo>n.hi),9223372036854775807) FROM native n), selected AS (SELECT m.id,m.role,m.seq,m.text,m.session_id FROM gaps g JOIN conversation_messages m ON m.session_id=?1 AND m.seq>g.lo AND m.seq<g.hi WHERE m.seq>?2 AND NOT {covered} UNION SELECT m.id,m.role,m.seq,m.text,m.session_id FROM json_each(?5) e JOIN conversation_messages m ON m.id=e.value AND m.session_id=?1) SELECT m.id,m.role,CASE WHEN {covered} THEN '' WHEN length(CAST(m.text AS BLOB))<=?4 THEN m.text END FROM selected m WHERE m.role IN('user','assistant') ORDER BY m.seq LIMIT 4097"
        );
        let mut stmt = conn.prepare(&sql)?;
        for row in stmt.query_map(
            params![session, after_seq, bounds, budget as i64, endpoints],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )? {
            let (id, role, text) = row?;
            let text = text.ok_or(StorageError::CompressionConflict)?;
            if rows.len() >= 4096 || text.len() > budget {
                return Err(StorageError::CompressionConflict);
            }
            budget -= text.len();
            rows.push((id, role, text));
        }
        for block in &mut graph {
            block.members = vec![block.start_msg.clone()];
            if block.end_msg != block.start_msg {
                block.members.push(block.end_msg.clone());
            }
        }
        let prune = conn
            .query_row(
                "SELECT up_to_msg FROM prune_marks WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        let high: i64 = conn.query_row(
            "SELECT high_water FROM compression_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        Ok((
            rows,
            graph,
            prune,
            (high.max(0) as u64).saturating_add(1),
            Self::dcp_projection_revision_in(&conn, session)?,
        ))
    }

    /// Only decisions for occurrences actually present in this bounded wire.
    /// SQL may inspect indexed archival rows; no lifetime set crosses into Rust.
    pub(crate) fn dcp_tool_projection_for_input(
        &self,
        session: &str,
        input: &[crate::provider::InputItem],
    ) -> Result<DcpToolProjection, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::dcp_tool_projection_for_input_in(&conn, session, input)
    }

    fn dcp_tool_projection_for_input_in(
        conn: &Connection,
        session: &str,
        input: &[crate::provider::InputItem],
    ) -> Result<DcpToolProjection, StorageError> {
        let keys = crate::runtime::dcp_call_contents(input, &DcpToolProjection::default())
            .into_keys()
            .collect::<Vec<_>>();
        Self::dcp_tool_projection_for_keys_in(conn, session, &keys)
    }
    pub(crate) fn dcp_tool_projection_for_keys(
        &self,
        session: &str,
        keys: &[DcpCallKey],
    ) -> Result<DcpToolProjection, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::dcp_tool_projection_for_keys_in(&conn, session, keys)
    }
    fn dcp_tool_projection_for_keys_in(
        conn: &Connection,
        session: &str,
        keys: &[DcpCallKey],
    ) -> Result<DcpToolProjection, StorageError> {
        if keys.len() > 4096 {
            return Err(StorageError::CompressionConflict);
        }
        let keys = serde_json::to_string(&keys).map_err(|_| StorageError::CompressionConflict)?;
        let mut projection = DcpToolProjection::default();
        for row in conn.prepare("SELECT p.call_id,p.occurrence,p.action FROM json_each(?2) k JOIN dcp_tool_projection_v2 p ON p.session_id=?1 AND p.call_id=k.value->>'$[0]' AND p.occurrence=k.value->>'$[1]' WHERE p.active=1 OR(p.active IS NULL AND NOT EXISTS(SELECT 1 FROM prefs WHERE key='dcp.projection_owned.'||?1 AND value='true'))")?.query_map(params![session,keys],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?)))? {
            let (id,n,action)=row?;
            let n=u64::try_from(n).map_err(|_|StorageError::CompressionConflict)?;
            match action.as_str() { "hidden" => {projection.hidden.insert((id,n));}, "purged" => {projection.purged.insert((id,n));}, _=>return Err(StorageError::CompressionConflict) }
        }
        Ok(projection)
    }

    pub(crate) fn dcp_block_count(&self, session: &str) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let n: i64 = conn.query_row("SELECT count(*) FROM compression_blocks cb WHERE cb.session_id=?1 AND (cb.hot IS NOT NULL AND json_valid(cb.hot) AND json_extract(cb.hot,'$.active')=1 OR cb.hot IS NULL AND EXISTS(SELECT 1 FROM compression_members cm WHERE cm.block_id=cb.id))",[session],|r|r.get(0))?;
        Ok(n.max(0) as usize)
    }

    /// At most two genuine in-window member references per active root. Legacy
    /// runtime consumers need an ordered endpoint, never the membership archive.
    pub(crate) fn block_window_endpoints(
        &self,
        session: &str,
        after_seq: i64,
    ) -> Result<Vec<(String, String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        // Joining the conversation view twice makes SQLite drive each join by
        // session_id alone (one full message-window scan per endpoint).
        // Correlated point lookups can use messages(session_id, seq) instead;
        // keep the view so staged undo/exclusions are still respected.
        let mut stmt=conn.prepare("WITH positions AS (SELECT b.id AS block_id,(SELECT MIN(m.seq) FROM conversation_messages m JOIN messages a ON a.id=b.start_msg JOIN messages z ON z.id=b.end_msg WHERE m.session_id=b.session_id AND m.seq>?2 AND m.seq BETWEEN a.seq AND z.seq) AS first,(SELECT seq FROM conversation_messages z WHERE z.id=b.end_msg AND z.seq>?2) AS last FROM compression_blocks b WHERE b.session_id=?1 AND b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 AND json_extract(b.hot,'$.standalone')=1 UNION ALL SELECT cm.block_id,MIN(m.seq) AS first,MAX(m.seq) AS last FROM compression_members cm JOIN conversation_messages m ON m.id=cm.message_id JOIN compression_blocks b ON b.id=cm.block_id WHERE b.session_id=?1 AND m.seq>?2 AND (b.hot IS NULL OR json_extract(b.hot,'$.active')=1) AND COALESCE(json_extract(b.hot,'$.standalone'),0)=0 GROUP BY cm.block_id) SELECT p.block_id,(SELECT a.id FROM conversation_messages a WHERE a.session_id=?1 AND a.seq=p.first),(SELECT z.id FROM conversation_messages z WHERE z.session_id=?1 AND z.seq=p.last) FROM positions p WHERE first IS NOT NULL AND last IS NOT NULL LIMIT 4097")?;
        let endpoints = stmt
            .query_map(params![session, after_seq], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if endpoints.len() > 4096 {
            return Err(StorageError::CompressionConflict);
        }
        Ok(endpoints)
    }
    /// Existing nudge preferences are versioned with the conversation branch.
    pub(crate) fn dcp_nudges(&self, session: &str) -> Result<u64, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let count: i64 = conn.query_row("SELECT COALESCE(sum(CASE WHEN json_valid(value) THEN COALESCE(json_extract(value,'$.emitted'),0) ELSE 0 END),0) FROM prefs WHERE substr(CAST(key AS BLOB),1,length(CAST('dcp.nudge.'||?1||char(0) AS BLOB)))=CAST('dcp.nudge.'||?1||char(0) AS BLOB)",[session],|r|r.get(0))?;
        Ok(count.max(0) as u64)
    }
    pub(super) fn dcp_view_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dcp_accounting(session_id TEXT PRIMARY KEY REFERENCES sessions(id), snapshot TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS dcp_run_views(operation_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), snapshot TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS dcp_run_session ON dcp_run_views(session_id);
             CREATE TABLE IF NOT EXISTS dcp_coverage(session_id TEXT NOT NULL REFERENCES sessions(id), kind TEXT NOT NULL, identity TEXT NOT NULL, PRIMARY KEY(session_id,kind,identity));
             CREATE TABLE IF NOT EXISTS dcp_run_identity(session_id TEXT PRIMARY KEY REFERENCES sessions(id), high_water INTEGER NOT NULL);",
        )?;
        Ok(())
    }

    /// One compact current-branch accounting row; absent is honest legacy state.
    pub fn dcp_accounting(&self, session: &str) -> Result<Option<DcpAccounting>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        Self::dcp_accounting_in(&conn, session)
    }

    fn dcp_accounting_in(
        conn: &Connection,
        session: &str,
    ) -> Result<Option<DcpAccounting>, StorageError> {
        let raw: Option<String> = conn.query_row(
            "SELECT CASE WHEN length(CAST(snapshot AS BLOB))<=?2 THEN snapshot END FROM dcp_accounting WHERE session_id=?1",
            params![session,SNAPSHOT_CAP as i64], |r| r.get(0)).optional()?.flatten();
        Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
    }

    /// Operation-scoped compact snapshot. Nothing is derived from output text.
    pub fn dcp_run(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<Option<DcpRunSnapshot>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::dcp_run_in(&conn, session, operation)
    }

    pub(super) fn dcp_run_in(
        conn: &Connection,
        session: &str,
        operation: &str,
    ) -> Result<Option<DcpRunSnapshot>, StorageError> {
        let raw = conn.query_row(
            "SELECT CASE WHEN length(CAST(snapshot AS BLOB))<=?3 THEN snapshot END FROM dcp_run_views WHERE session_id=?1 AND operation_id=?2",
            params![session,operation,SNAPSHOT_CAP as i64], |r| r.get(0)).optional()?.flatten();
        Ok(decode(raw))
    }

    /// Read one bounded UTF-8 byte page from a real range summary. Block index
    /// is scoped by the operation snapshot, preventing foreign-block access.
    pub fn dcp_summary_page(
        &self,
        session: &str,
        operation: &str,
        block_index: usize,
        offset: i64,
        limit: usize,
    ) -> Result<Option<DcpSummaryPage>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let Some(run) = Self::dcp_run_in(&conn, session, operation)? else {
            return Ok(None);
        };
        let Some(block) = run.block_ids.get(block_index) else {
            return Ok(None);
        };
        let limit = limit.clamp(4, SUMMARY_PAGE_CAP);
        let offset = offset.max(0);
        let page = conn.query_row(
            "SELECT topic,length(CAST(summary AS BLOB)),substr(CAST(summary AS BLOB),?3+1,?4) FROM compression_blocks WHERE session_id=?1 AND id=?2",
            params![session,block,offset,limit as i64], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).optional()?;
        let Some((topic, total, bytes)) = page else {
            return Ok(None);
        };
        // A caller may supply an interior offset. Discard only leading partial
        // bytes, then return complete characters and a genuine next boundary.
        let leading = bytes.iter().take_while(|b| (**b & 0xc0) == 0x80).count();
        let bytes = &bytes[leading..];
        let kept = match std::str::from_utf8(bytes) {
            Ok(_) => bytes.len(),
            Err(e) => e.valid_up_to(),
        };
        let text = std::str::from_utf8(&bytes[..kept])
            .map_err(|_| StorageError::CompressionConflict)?
            .to_owned();
        let next = offset.saturating_add((leading + kept) as i64);
        Ok(Some(DcpSummaryPage {
            block_id: block.clone(),
            topic,
            text,
            total_bytes: total,
            next_offset: (next < total).then_some(next),
        }))
    }

    pub(super) fn commit_dcp_view(
        conn: &Connection,
        session: &str,
        blocks: &[CompressionBlockRow],
        measured: &crate::dcp::DcpMeasurement,
        tool: Option<&ToolOutcomeLogCommit<'_>>,
    ) -> Result<DcpRunSnapshot, StorageError> {
        let ids = serde_json::to_string(&blocks.iter().map(|b| &b.id).collect::<Vec<_>>())
            .map_err(|_| StorageError::CompressionConflict)?;
        let mut accounting = match Self::dcp_accounting_in(conn, session)? {
            Some(value) => value,
            None => {
                let legacy: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM compression_blocks WHERE session_id=?1 AND id NOT IN (SELECT value FROM json_each(?2))) OR EXISTS(SELECT 1 FROM prune_marks WHERE session_id=?1)",params![session,ids],|r|r.get(0))?;
                DcpAccounting {
                    complete: !legacy,
                    ..Default::default()
                }
            }
        };
        let mut new_messages = 0u64;
        let mut recent_messages = Vec::new();
        let mut new_tools = 0u64;
        for message in &measured.messages {
            let inserted = conn.execute(
                "INSERT OR IGNORE INTO dcp_coverage VALUES(?1,'message',?2)",
                params![session, message],
            )? as u64;
            new_messages += inserted;
            if inserted > 0 {
                recent_messages.push(message);
            }
        }
        for call in &measured.calls {
            let owned: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2)",
                params![call.0, session],
                |r| r.get(0),
            )?;
            if !owned {
                return Err(StorageError::CompressionConflict);
            }
            new_tools += conn.execute(
                "INSERT OR IGNORE INTO dcp_coverage VALUES(?1,'call',?2)",
                params![
                    session,
                    serde_json::to_string(call).map_err(|_| StorageError::CompressionConflict)?
                ],
            )? as u64;
        }
        accounting.gross_removed = accounting.gross_removed.saturating_add(measured.removed);
        accounting.complete &= !measured.unavailable_coverage;
        accounting.active_summary = measured.active_summary;
        accounting.net_saved = accounting.net_saved.saturating_add(measured.net_saved);
        accounting.compressions = accounting.compressions.saturating_add(1);
        accounting.prunes = accounting.prunes.saturating_add(measured.prunes);
        let ordinal: i64 = conn.query_row("INSERT INTO dcp_run_identity VALUES(?1,1) ON CONFLICT(session_id) DO UPDATE SET high_water=high_water+1 RETURNING high_water",[session],|r|r.get(0))?;
        let operation_id = tool
            .map(|t| t.operation_id.to_owned())
            .unwrap_or_else(|| format!("dcp:{session}:{ordinal}"));
        if tool.is_none() {
            // The low-level/manual helper still has one real durable operation;
            // it never adds a conversation message or a parallel history log.
            conn.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES(?1,?2,NULL,'compress','completed',NULL,NULL)",params![operation_id,session])?;
        }
        let snapshot = DcpRunSnapshot {
            session: session.into(),
            operation_id: operation_id.clone(),
            ordinal: ordinal as u64,
            topic: blocks.first().map(|b| b.topic.clone()).unwrap_or_default(),
            block_ids: blocks.iter().map(|b| b.id.clone()).collect(),
            removed: measured.removed,
            summary: measured.summary,
            net_saved: measured.net_saved,
            method: Default::default(),
            new_messages,
            new_tools,
            cumulative: accounting.clone(),
            bar: Self::dcp_bar(
                conn,
                session,
                &serde_json::to_string(&recent_messages)
                    .map_err(|_| StorageError::CompressionConflict)?,
            )?,
        };
        let raw =
            serde_json::to_string(&snapshot).map_err(|_| StorageError::CompressionConflict)?;
        if raw.len() > SNAPSHOT_CAP {
            return Err(StorageError::CompressionConflict);
        }
        conn.execute(
            "INSERT INTO dcp_run_views VALUES(?1,?2,?3)",
            params![operation_id, session, raw],
        )?;
        conn.execute("INSERT INTO dcp_accounting VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET snapshot=excluded.snapshot",params![session,serde_json::to_string(&accounting).map_err(|_| StorageError::CompressionConflict)?])?;
        Ok(snapshot)
    }

    /// Effective prefix pruning is counted against projected content, not raw
    /// covered payloads. The mark and accounting share the caller's transaction.
    pub(super) fn account_prune(
        conn: &Connection,
        session: &str,
        old_seq: i64,
        new_seq: i64,
    ) -> Result<(), StorageError> {
        if new_seq <= old_seq {
            return Ok(());
        }
        let graph = Self::active_compression_graph_in(conn, session, old_seq)?;
        let by_id = graph
            .iter()
            .map(|b| (b.id.clone(), b.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let (rows,bytes): (i64,i64) = conn.query_row("SELECT count(*),COALESCE(sum(length(CAST(text AS BLOB))),0) FROM conversation_messages m WHERE m.session_id=?1 AND m.seq>?2 AND m.seq<=?3 AND NOT EXISTS(SELECT 1 FROM compression_members cm WHERE cm.message_id=m.id)",params![session,old_seq,new_seq],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if rows > 4096 || bytes > crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64 {
            return Err(StorageError::CompressionConflict);
        }
        let estimate = oc_core::dcp_view::estimate_content;
        let texts = conn.prepare("SELECT id,role,text FROM conversation_messages m WHERE m.session_id=?1 AND m.seq>?2 AND m.seq<=?3 AND NOT EXISTS(SELECT 1 FROM compression_members cm WHERE cm.message_id=m.id)")?.query_map(params![session,old_seq,new_seq],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
        let logs = Self::presentation_wire_logs_in(conn, session, old_seq, &texts, &graph)?
            .ok_or(StorageError::CompressionConflict)?;
        let mut wire = Vec::new();
        let mut represented = std::collections::BTreeSet::new();
        for raw in &logs {
            let value: serde_json::Value =
                serde_json::from_str(raw).map_err(|_| StorageError::CompressionConflict)?;
            let log = crate::tools::TurnLog::from_json(&value)
                .map_err(|_| StorageError::CompressionConflict)?;
            let Some(anchor) = log.user_message else {
                continue;
            };
            let seq: i64 = conn.query_row(
                "SELECT seq FROM conversation_messages WHERE session_id=?1 AND id=?2",
                params![session, anchor],
                |r| r.get(0),
            )?;
            if seq > new_seq {
                continue;
            }
            represented.insert(anchor);
            let assistant_survives = if let Some(assistant) = value["assistant_message"].as_str() {
                represented.insert(assistant.into());
                conn.query_row(
                    "SELECT seq>?3 FROM conversation_messages WHERE session_id=?1 AND id=?2",
                    params![session, assistant, new_seq],
                    |r| r.get::<_, bool>(0),
                )
                .optional()?
                .unwrap_or(false)
            } else {
                false
            };
            let answered = log
                .input
                .iter()
                .filter_map(|i| match i {
                    crate::provider::InputItem::FunctionCallOutput { call_id, .. }
                    | crate::provider::InputItem::McpFunctionCallOutput { call_id, .. }
                    | crate::provider::InputItem::ReadFunctionCallOutput { call_id, .. } => {
                        Some(call_id.clone())
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>();
            wire.extend(log.input.into_iter().filter(|i| {
                match i {
                    crate::provider::InputItem::ProviderOutput(v)
                        if v["type"] == "function_call" =>
                    {
                        v["call_id"]
                            .as_str()
                            .is_some_and(|id| answered.contains(id))
                    }
                    crate::provider::InputItem::ProviderOutput(v)
                        if assistant_survives && v["type"] == "message" =>
                    {
                        false
                    }
                    _ => true,
                }
            }));
        }
        for (id, _, text) in &texts {
            if !represented.contains(id) {
                wire.push(crate::provider::InputItem::message(
                    crate::provider::InputRole::User,
                    text,
                ));
            }
        }
        let projection = Self::dcp_tool_projection_for_input_in(conn, session, &wire)?;
        let identities = crate::runtime::dcp_call_identities(&logs, None)
            .map_err(|_| StorageError::CompressionConflict)?;
        let calls = crate::runtime::dcp_call_contents(&wire, &projection);
        crate::runtime::apply_dcp_projection(&mut wire, &projection);
        let gross_removed: u64 = crate::runtime::dcp_contents(&wire)
            .iter()
            .map(|text| estimate(text))
            .sum();
        let mut effective = crate::runtime::dcp_contents(&wire)
            .iter()
            .any(|text| !text.is_empty());
        let mut removed = gross_removed;
        let mut active_summary = 0u64;
        // A summary remains active if any member survives the new prune bound.
        for block in &graph {
            let (old_active,new_active): (bool,bool) = conn.query_row("SELECT EXISTS(SELECT 1 FROM compression_members cm JOIN conversation_messages m ON m.id=cm.message_id WHERE cm.block_id=?1 AND m.seq>?2),EXISTS(SELECT 1 FROM compression_members cm JOIN conversation_messages m ON m.id=cm.message_id WHERE cm.block_id=?1 AND m.seq>?3)",params![block.id,old_seq,new_seq],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if !old_active {
                continue;
            }
            let summary = crate::dcp::expand_block(&by_id, &block.id, 0, &mut Vec::new())
                .map_err(|_| StorageError::CompressionConflict)?;
            if new_active {
                active_summary += estimate(&summary);
            } else {
                effective = true;
                removed += estimate(&format!("[compressed {}] {summary}", block.id));
            }
        }
        if !effective {
            return Ok(());
        }
        let mut accounting = Self::dcp_accounting_in(conn, session)?.unwrap_or(DcpAccounting {
            complete: graph.is_empty() && old_seq == 0,
            ..Default::default()
        });
        accounting.gross_removed = accounting.gross_removed.saturating_add(gross_removed);
        accounting.net_saved = accounting.net_saved.saturating_add(removed);
        accounting.active_summary = active_summary;
        accounting.prunes += 1;
        conn.execute("INSERT INTO dcp_accounting VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET snapshot=excluded.snapshot",params![session,serde_json::to_string(&accounting).map_err(|_| StorageError::CompressionConflict)?])?;
        // Keep actual prefix coverage in the same revision, SQL-streamed.
        conn.execute("INSERT OR IGNORE INTO dcp_coverage SELECT session_id,'message',id FROM conversation_messages WHERE session_id=?1 AND seq>?2 AND seq<=?3",params![session,old_seq,new_seq])?;
        for (call, (input, output)) in calls {
            if !input.is_empty() || !output.is_empty() {
                let Some(identity) = identities.get(&call) else {
                    continue;
                };
                conn.execute(
                    "INSERT OR IGNORE INTO dcp_coverage VALUES(?1,'call',?2)",
                    params![
                        session,
                        serde_json::to_string(identity)
                            .map_err(|_| StorageError::CompressionConflict)?
                    ],
                )?;
            }
        }
        Ok(())
    }

    // Source-derived D06 mapping: floor(m/total*50)..floor((m+1)/total*50).
    // DCP 3.1.15 11f6517780a502512a3467645074be447cb0369e/lib/ui/utils.ts,
    // AGPL-3.0-or-later; provenance in docs/DCP_VIS38_PROVENANCE.md. SQL samples only
    // 50 canonical positions and never fetches message contents/archive rows.
    fn dcp_bar(
        conn: &Connection,
        session: &str,
        recent_messages: &str,
    ) -> Result<String, StorageError> {
        let total: i64 = conn.query_row(
            "SELECT count(*) FROM conversation_messages WHERE session_id=?1",
            [session],
            |r| r.get(0),
        )?;
        if total == 0 {
            return Ok("░".repeat(50));
        }
        let mut bar = String::new();
        let mut stmt = conn.prepare_cached("SELECT CASE WHEN m.id IN (SELECT value FROM json_each(?3)) THEN '⣿' WHEN EXISTS(SELECT 1 FROM compression_members cm JOIN compression_blocks cb ON cb.id=cm.block_id WHERE cm.message_id=m.id AND cb.session_id=?1) OR m.seq<=COALESCE((SELECT seq FROM messages WHERE session_id=?1 AND id=(SELECT up_to_msg FROM prune_marks WHERE session_id=?1)),0) THEN '░' ELSE '█' END FROM conversation_messages m WHERE session_id=?1 ORDER BY seq LIMIT 1 OFFSET ?2")?;
        for cell in 0..50i64 {
            let position = ((cell + 1) * total + 49) / 50 - 1;
            let category: String =
                stmt.query_row(params![session, position, recent_messages], |r| r.get(0))?;
            bar.push_str(&category);
        }
        Ok(bar)
    }
}

#[cfg(test)]
#[path = "storage_dcp_view_tests.rs"]
mod tests;

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::provider::{InputItem, InputRole};

    #[test]
    fn equal_active_context_does_not_load_large_inactive_archive_or_decisions() {
        let data = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        db.apply_dcp_schema().unwrap();
        let old = db
            .append_message("s", "user", &"inactive archive ".repeat(400_000))
            .unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute("INSERT INTO prune_marks VALUES('s',?1,'fixture')", [&old])
                .unwrap();
        }
        let user = db
            .accept_turn(
                "active",
                "s",
                "same active",
                "same active",
                &oc_core::queries::ModelRef {
                    provider: "test".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap()
            .user_message;
        let mut log = crate::tools::TurnLog::new("active", "m", "test");
        log.user_message = Some(user.clone());
        log.input
            .push(InputItem::message(InputRole::User, "same active"));
        db.commit_turn(
            "active",
            "completed",
            Some(&log.to_json().to_string()),
            Some("same answer"),
        )
        .unwrap();
        let floor = db.prune_bound("s").unwrap().unwrap().1;
        let active = db
            .active_history("s", floor, crate::runtime::ACTIVE_CONTEXT_BYTES_CAP)
            .unwrap()
            .rows;
        let before = db
            .presentation_wire_logs("s", floor, &active, &[])
            .unwrap()
            .unwrap();
        let wire = vec![
            InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","call_id":"active","name":"read","arguments":"{}"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "active".into(),
                output: "real output".into(),
            },
        ];
        let projection = db.dcp_tool_projection_for_input("s", &wire).unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<12000) INSERT INTO dcp_tool_projection_v2(session_id,call_id,occurrence,action) SELECT 's','inactive-'||x,0,'hidden' FROM n",[]).unwrap();
            conn.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<5000) INSERT INTO compression_blocks(id,session_id,topic,summary,start_msg,end_msg,created_at) SELECT 'b'||printf('%04d',x),'s','inactive','never active',?1,?1,'fixture' FROM n",[&old]).unwrap();
            conn.execute("INSERT INTO compression_members SELECT id,?1 FROM compression_blocks WHERE session_id='s'",[&old]).unwrap();
        }
        assert_eq!(
            db.presentation_wire_logs(
                "s",
                floor,
                &active,
                &db.active_compression_graph("s", floor).unwrap()
            )
            .unwrap()
            .unwrap(),
            before
        );
        assert_eq!(
            db.dcp_tool_projection_for_input("s", &wire).unwrap(),
            projection
        );
        let tail = db.append_message("s", "user", "tail").unwrap();
        let range = crate::dcp::ValidatedRange {
            topic: "bounded".into(),
            start_id: user.clone(),
            end_id: active.last().unwrap().0.clone(),
            summary: "short".into(),
        };
        let (rows, graph, _, _, _) = db
            .compression_addressed_snapshot("s", floor, &[range])
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.last().unwrap().0, tail);
        assert!(graph.is_empty());
        // Pruning the equal active prefix still succeeds beside oversized old
        // raw content and thousands of inactive projection/graph rows.
        db.save_prune_mark("s", &user).unwrap();
        assert_eq!(
            db.dcp_accounting("s").unwrap().unwrap().gross_removed,
            oc_core::dcp_view::estimate_content("same active")
        );
        let end = db
            .append_message("s", "assistant", &"active removable answer ".repeat(4096))
            .unwrap();
        db.append_message("s", "user", "new unfinished tail")
            .unwrap();
        let project = tempfile::tempdir().unwrap();
        let runtime = crate::runtime::Runtime::new(
            &db,
            "work",
            crate::config::Generation {
                permissions: [("compress".into(), crate::config::Permission::Allow)]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            crate::patch::ProtectedGlobs { patterns: vec![] },
            crate::files::Files::new(project.path(), data.path()).unwrap(),
            crate::shell::Shell::new(project.path()).unwrap(),
            Default::default(),
            crate::tools::ToolRoots {
                project: project.path().into(),
                data: data.path().into(),
            },
            None,
            false,
            crate::dcp_auto::DcpConfig::default(),
        )
        .unwrap();
        let result=runtime.run_compress("s",&serde_json::json!({"topic":"bounded manual","content":[{"startId":tail,"endId":end,"summary":"short"}]}),&oc_core::context_plan::ProtectedSpec::default()).unwrap();
        assert!(result.shrank);
        assert_eq!(db.dcp_accounting("s").unwrap().unwrap().compressions, 1);
    }
}
