//! Import genuine historical metadata into a fork's own revision timeline.
//! Immutable row objects are shared across points; only changed rows receive
//! new validity intervals. The live source tables are never used or modified.
use super::*;
use std::collections::BTreeMap;

struct Import<'a> {
    conn: &'a Connection,
    source: &'a str,
    root: &'a str,
    messages: &'a HashMap<String, String>,
    turns: &'a HashMap<String, String>,
    operations: HashMap<String, String>,
    blocks: HashMap<String, String>,
    objects: HashMap<i64, (i64, String)>,
    contexts: HashMap<String, String>,
    bytes: i64,
    rows: i64,
}

fn mapped(map: &HashMap<String, String>, value: &serde_json::Value) -> Result<String, ForkError> {
    value
        .as_str()
        .and_then(|v| map.get(v))
        .cloned()
        .ok_or_else(|| refuse("historical context references outside copied prefix").into())
}

/// The same genuine cuts that `copy` imports may justify standalone compress
/// operations. Nothing in today's live DCP tables, future points or tool names
/// alone can exempt an unanchored tool from the fork refusal.
pub(super) fn standalone_prefix_operations(
    conn: &Connection,
    source: &str,
    before: &str,
    cutoff: i64,
) -> Result<HashSet<String>, ForkError> {
    let mut digests = HashSet::new();
    let mut points = conn.prepare("SELECT p.pre_context,p.post_context FROM conversation_points p JOIN conversation_turns t ON t.id=p.turn_id AND t.session_id=p.session_id JOIN turn_acceptances a ON a.turn_id=p.turn_id AND a.session_id=p.session_id JOIN conversation_messages m ON m.id=a.user_message AND m.session_id=p.session_id WHERE p.session_id=?1 AND p.active=1 AND m.seq<?2 LIMIT 4097")?;
    for (index, row) in points
        .query_map(params![source, cutoff], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .enumerate()
    {
        if index >= super::MAX_ROWS as usize {
            return Err(refuse("copy budget exceeded").into());
        }
        let (pre, post) = row?;
        digests.insert(pre);
        digests.extend(post);
    }
    let boundary: Option<String> = conn.query_row("SELECT p.pre_context FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id AND a.session_id=p.session_id WHERE p.session_id=?1 AND a.user_message=?2 AND p.active=1",params![source,before],|r|r.get(0)).optional()?;
    digests.extend(boundary);
    let mut allowed = HashSet::new();
    for digest in digests {
        Db::load_context_restore(conn, source, &digest)?;
        let mut runs = conn.prepare("SELECT json_extract(o.payload,'$[0]'),json_extract(o.payload,'$[2]') FROM conversation_restore r JOIN conversation_objects o ON o.id=r.object_id JOIN tool_operations t ON t.id=json_extract(o.payload,'$[0]') AND t.session_id=?1 WHERE r.kind=9 AND t.turn_id IS NULL AND t.name='compress' AND t.state='completed' AND json_extract(o.payload,'$[1]')=?1 AND length(CAST(o.payload AS BLOB))<=32768 LIMIT 4097")?;
        for (index, row) in runs
            .query_map([source], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .enumerate()
        {
            if index >= super::MAX_ROWS as usize {
                return Err(refuse("copy budget exceeded").into());
            }
            let (operation, raw) = row?;
            let run: oc_core::dcp_view::DcpRunSnapshot = serde_json::from_str(&raw)
                .map_err(|_| refuse("invalid historical DCP snapshot"))?;
            if run.session != source || run.operation_id != operation || run.ordinal == 0 {
                return Err(refuse("invalid historical DCP operation scope").into());
            }
            allowed.insert(operation);
            if allowed.len() > super::MAX_ROWS as usize {
                return Err(refuse("copy budget exceeded").into());
            }
        }
    }
    conn.execute("DELETE FROM conversation_restore", [])?;
    Ok(allowed)
}

impl Import<'_> {
    fn selection(&self, value: &mut serde_json::Value) -> Result<(), ForkError> {
        let logs = value
            .as_array_mut()
            .ok_or_else(|| refuse("invalid historical selection"))?;
        for log in logs {
            if let Some(facts) = log.get_mut("legacy_protected") {
                for fact in facts
                    .as_array_mut()
                    .ok_or_else(|| refuse("invalid legacy protection"))?
                {
                    if !matches!(fact["role"].as_str(), Some("user" | "content"))
                        || !fact["text"].is_string()
                    {
                        return Err(refuse("invalid legacy protection").into());
                    }
                    fact["source_start"] = mapped(self.messages, &fact["source_start"])?.into();
                    fact["source_end"] = mapped(self.messages, &fact["source_end"])?.into();
                }
                continue;
            }
            if let Some(fact) = log.get_mut("protected_message") {
                fact["id"] = mapped(self.messages, &fact["id"])?.into();
                if !matches!(fact["role"].as_str(), Some("user" | "assistant"))
                    || !fact["text"].is_string()
                {
                    return Err(refuse("invalid historical protected message").into());
                }
                continue;
            }
            crate::tools::TurnLog::from_json(log)
                .map_err(|_| refuse("invalid historical selected journal"))?;
            log["turn_id"] = mapped(self.turns, &log["turn_id"])?.into();
            if !log["user_message"].is_null() {
                log["user_message"] = mapped(self.messages, &log["user_message"])?.into();
            }
            if let Some(notices) = log["shell_notice_messages"].as_array_mut() {
                for notice in notices {
                    *notice = mapped(self.messages, notice)?.into();
                }
            }
        }
        Ok(())
    }
    fn context(&mut self, digest: &str) -> Result<String, ForkError> {
        if let Some(saved) = self.contexts.get(digest) {
            return Ok(saved.clone());
        }
        Db::load_context_restore(self.conn, self.source, digest)?;
        self.conn.execute("DELETE FROM fork_restore", [])?;
        let mut stmt = self.conn.prepare("SELECT o.id,o.kind,o.payload FROM conversation_restore r JOIN conversation_objects o ON o.id=r.object_id ORDER BY o.kind,o.id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let old: i64 = row.get(0)?;
            let kind: i64 = row.get(1)?;
            let (object, key) = if let Some(cached) = self.objects.get(&old) {
                cached.clone()
            } else {
                let raw: String = row.get(2)?;
                let mut v: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|_| refuse("invalid historical metadata"))?;
                match kind {
                    0 => {
                        let old_block = v[0]
                            .as_str()
                            .ok_or_else(|| refuse("invalid historical block"))?
                            .to_owned();
                        let block = if let Some(block) = self.blocks.get(&old_block) {
                            block.clone()
                        } else {
                            // Reserve even blocks absent at the initial cut: a
                            // later Undo must never collide with another root.
                            let n: i64 = self.conn.query_row("UPDATE compression_identity SET high_water=high_water+1 WHERE singleton=1 RETURNING high_water",[],|r|r.get(0))?;
                            let block = format!("b{n:04}");
                            self.blocks.insert(old_block, block.clone());
                            block
                        };
                        v[0] = block.into();
                        v[1] = self.root.into();
                        let summary = v[3]
                            .as_str()
                            .ok_or_else(|| refuse("invalid historical summary"))?;
                        let authored = crate::dcp::authored_summary(summary);
                        let mut rebased = String::new();
                        let mut end = 0;
                        let mut hot = v
                            .get(7)
                            .filter(|v| !v.is_null())
                            .map(|v| match v {
                                serde_json::Value::Object(_) => Ok(v.clone()),
                                serde_json::Value::String(raw) => {
                                    serde_json::from_str::<serde_json::Value>(raw)
                                        .map_err(|_| refuse("invalid historical hot metadata"))
                                }
                                _ => Err(refuse("invalid historical hot metadata")),
                            })
                            .transpose()?;
                        let standalone = hot.as_ref().is_some_and(|h| h["standalone"] == true);
                        for placeholder in crate::dcp::parse_block_placeholders(if standalone {
                            ""
                        } else {
                            authored
                        }) {
                            let target = if let Some(id) = self.blocks.get(&placeholder.block_id) {
                                id.clone()
                            } else {
                                let present: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM fork_objects f JOIN conversation_objects o ON o.id=f.id WHERE o.kind=0 AND json_extract(o.payload,'$[0]')=?1)",[&placeholder.block_id],|r|r.get(0))?;
                                if !present {
                                    return Err(refuse(
                                        "historical summary reference outside prefix",
                                    )
                                    .into());
                                }
                                let n: i64 = self.conn.query_row("UPDATE compression_identity SET high_water=high_water+1 WHERE singleton=1 RETURNING high_water",[],|r|r.get(0))?;
                                let id = format!("b{n:04}");
                                self.blocks.insert(placeholder.block_id.clone(), id.clone());
                                id
                            };
                            rebased.push_str(&authored[end..placeholder.start]);
                            rebased.push_str(&format!("({target})"));
                            end = placeholder.start + placeholder.raw.len();
                        }
                        rebased.push_str(&summary[end..]); // protected verbatim suffix is untouched.
                        v[3] = rebased.into();
                        v[4] = mapped(self.messages, &v[4])?.into();
                        v[5] = mapped(self.messages, &v[5])?.into();
                        if let Some(hot) = &mut hot {
                            for fact in hot["protected"]
                                .as_array_mut()
                                .ok_or_else(|| refuse("invalid protected selection"))?
                            {
                                fact["id"] = mapped(self.messages, &fact["id"])?.into();
                            }
                            if let Some(facts) = hot.get("legacy_protected") {
                                let mut selection = serde_json::json!([{"legacy_protected":facts}]);
                                self.selection(&mut selection)?;
                                hot["legacy_protected"] = selection[0]["legacy_protected"].take();
                            }
                            self.selection(&mut hot["logs"])?;
                            v[7] = hot.to_string().into();
                        }
                    }
                    1 => {
                        v[0] = mapped(&self.blocks, &v[0])?.into();
                        v[1] = mapped(self.messages, &v[1])?.into();
                    }
                    2 => {
                        v[0] = self.root.into();
                        v[1] = mapped(self.messages, &v[1])?.into();
                    }
                    3 | 4 => {
                        v[0] = self.root.into();
                        // call_id is a provider wire identity, not an operation
                        // primary key. Preserve it even if strings coincide.
                    }
                    5 => {
                        if v[0].as_str() == Some(&format!("dcp.projection_owned.{}", self.source)) {
                            v[0] = format!("dcp.projection_owned.{}", self.root).into();
                        } else {
                            let suffix = v[0]
                                .as_str()
                                .and_then(|key| {
                                    key.strip_prefix(&format!("dcp.nudge.{}\0", self.source))
                                })
                                .ok_or_else(|| refuse("invalid historical nudge scope"))?;
                            v[0] = format!("dcp.nudge.{}\0{suffix}", self.root).into();
                        }
                    }
                    6 => {
                        v[0] = self.root.into();
                        v[1] = mapped(self.messages, &v[1])?.into();
                        let original = v[5]
                            .as_str()
                            .ok_or_else(|| refuse("invalid historical compaction operation"))?;
                        let new_id = format!("{}:{original}", self.root);
                        let present: bool = self.conn.query_row(
                            "SELECT EXISTS(SELECT 1 FROM session_compactions WHERE id=?1)",
                            [&new_id],
                            |r| r.get(0),
                        )?;
                        if !present {
                            let raw: String = self.conn.query_row(
                                "SELECT snapshot FROM session_compactions WHERE id=?1",
                                [original],
                                |r| r.get(0),
                            )?;
                            let mut snapshot: oc_core::compaction::CompactionSnapshot =
                                serde_json::from_str(&raw).map_err(|_| {
                                    refuse("invalid historical compaction presentation")
                                })?;
                            snapshot.id = new_id.clone();
                            snapshot.session = self.root.into();
                            snapshot.anchor.message = snapshot
                                .anchor
                                .message
                                .as_ref()
                                .and_then(|id| self.messages.get(id).cloned())
                                .or_else(|| v[1].as_str().map(String::from));
                            // Tool/turn archive identities are independently rebased by the fork.
                            // Public placement falls back to the genuine copied message anchor.
                            snapshot.anchor.turn = None;
                            snapshot.anchor.tool = None;
                            let raw = serde_json::to_string(&snapshot).map_err(|_| {
                                refuse("invalid historical compaction presentation")
                            })?;
                            self.bytes -= raw.len() as i64;
                            if self.bytes < 0 {
                                return Err(refuse("copy budget exceeded").into());
                            }
                            self.conn.execute(
                                "INSERT INTO session_compactions VALUES(?1,?2,?3)",
                                params![new_id, self.root, raw],
                            )?;
                        }
                        v[5] = new_id.into();
                        if let Some(raw) = v.get(6).filter(|v| !v.is_null()) {
                            let mut selection: serde_json::Value = match raw {
                                serde_json::Value::Array(_) => raw.clone(),
                                serde_json::Value::String(raw) => serde_json::from_str(raw)
                                    .map_err(|_| {
                                        refuse("invalid historical checkpoint selection")
                                    })?,
                                _ => {
                                    return Err(
                                        refuse("invalid historical checkpoint selection").into()
                                    );
                                }
                            };
                            self.selection(&mut selection)?;
                            v[6] = selection.to_string().into();
                        }
                    }
                    7 => {
                        // Wire identities and the measured causal prefix are unchanged.
                        v[0] = self.root.into();
                    }
                    8 => {
                        v[0] = self.root.into();
                    }
                    9 => {
                        let old_op = v[0]
                            .as_str()
                            .ok_or_else(|| refuse("invalid DCP operation"))?;
                        let mut run: oc_core::dcp_view::DcpRunSnapshot = serde_json::from_str(
                            v[2].as_str()
                                .ok_or_else(|| refuse("invalid DCP snapshot"))?,
                        )
                        .map_err(|_| refuse("invalid DCP snapshot"))?;
                        if run.session != self.source
                            || run.operation_id != old_op
                            || run.ordinal == 0
                        {
                            return Err(refuse("invalid historical DCP operation scope").into());
                        }
                        let new_op = match self.operations.get(old_op).cloned() {
                            Some(op) => op,
                            None => {
                                // Existing standalone/manual operations have no
                                // accepted-turn association. Import only those
                                // referenced by this genuine saved context cut.
                                let remaining = self.bytes.min(super::MAX_BYTES);
                                let row: Option<(Option<String>,Option<String>,i64)> = self.conn.query_row("SELECT input,output,COALESCE(length(CAST(input AS BLOB)),0)+COALESCE(length(CAST(output AS BLOB)),0) FROM tool_operations WHERE id=?1 AND session_id=?2 AND turn_id IS NULL AND name='compress' AND state='completed' AND COALESCE(length(CAST(input AS BLOB)),0)+COALESCE(length(CAST(output AS BLOB)),0)<=?3",params![old_op,self.source,remaining],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
                                let Some((input, output, bytes)) = row else {
                                    return Err(
                                        refuse("historical DCP operation outside prefix").into()
                                    );
                                };
                                if self.rows == 0 {
                                    return Err(refuse("DCP operation metadata limit").into());
                                }
                                self.rows -= 1;
                                self.bytes -= bytes;
                                let op = format!("{}:dcp-op:{}", self.root, self.operations.len());
                                self.conn.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES(?1,?2,NULL,'compress','completed',?3,?4)",params![op,self.root,input,output])?;
                                self.operations.insert(old_op.into(), op.clone());
                                op
                            }
                        };
                        run.session = self.root.into();
                        run.operation_id = new_op.clone();
                        for block in &mut run.block_ids {
                            *block = self
                                .blocks
                                .get(block)
                                .cloned()
                                .ok_or_else(|| refuse("historical DCP block outside prefix"))?;
                        }
                        let ordinal = i64::try_from(run.ordinal)
                            .map_err(|_| refuse("invalid DCP ordinal"))?;
                        self.conn.execute("INSERT INTO dcp_run_identity VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET high_water=MAX(high_water,excluded.high_water)",params![self.root,ordinal])?;
                        v[0] = new_op.into();
                        v[1] = self.root.into();
                        v[2] = serde_json::to_string(&run)
                            .map_err(|_| refuse("invalid DCP snapshot"))?
                            .into();
                    }
                    10 => {
                        v[0] = self.root.into();
                        if v[1].as_str() == Some("message") {
                            v[2] = mapped(self.messages, &v[2])?.into();
                        } else if v[1].as_str() == Some("call") {
                            let raw = v[2]
                                .as_str()
                                .ok_or_else(|| refuse("invalid DCP coverage identity"))?;
                            let mut identity: serde_json::Value = serde_json::from_str(raw)
                                .map_err(|_| refuse("invalid DCP coverage identity"))?;
                            // Immutable [turn_id, call_id, turn-local occurrence].
                            // Two-field historical wire-window keys are legacy;
                            // they are never reinterpreted as local archive IDs.
                            if identity.as_array().is_some_and(|a| a.len() == 3) {
                                if identity[1].as_str().is_none() || identity[2].as_u64().is_none()
                                {
                                    return Err(refuse("invalid DCP coverage identity").into());
                                }
                                identity[0] = mapped(self.turns, &identity[0])?.into();
                                v[2] = identity.to_string().into();
                            }
                        }
                    }
                    _ => return Err(refuse("unknown historical metadata").into()),
                }
                let payload = v.to_string();
                // Account for rebased identifier growth as well as source bytes.
                self.bytes -= (payload.len() as i64 - raw.len() as i64).max(0);
                if self.bytes < 0 {
                    return Err(refuse("copy budget exceeded").into());
                }
                let keys = match kind {
                    1 | 3 => 2,
                    4 | 10 => 3,
                    _ => 1,
                };
                let key = serde_json::Value::Array(
                    v.as_array()
                        .ok_or_else(|| refuse("invalid historical row"))?[..keys]
                        .to_vec(),
                )
                .to_string();
                self.conn.execute(
                    "INSERT OR IGNORE INTO conversation_objects(kind,payload) VALUES (?1,?2)",
                    params![kind, payload],
                )?;
                let object = self.conn.query_row(
                    "SELECT id FROM conversation_objects WHERE kind=?1 AND payload=?2",
                    params![kind, payload],
                    |r| r.get(0),
                )?;
                self.objects.insert(old, (object, key.clone()));
                (object, key)
            };
            self.conn.execute(
                "INSERT INTO fork_restore VALUES (?1,?2,?3)",
                params![kind, object, key],
            )?;
        }
        drop(rows);
        drop(stmt);
        self.conn.execute_batch("DELETE FROM conversation_restore; INSERT INTO conversation_restore SELECT * FROM fork_restore;")?;
        Db::apply_context_restore(self.conn, self.root)?;
        let saved = Db::save_context(self.conn, self.root)?;
        self.contexts.insert(digest.to_owned(), saved.clone());
        Ok(saved)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn copy(
    conn: &Connection,
    source: &str,
    before: &str,
    root: &str,
    messages: &HashMap<String, String>,
    turns: &HashMap<String, String>,
    operations: &HashMap<String, String>,
    seqs: &BTreeMap<i64, i64>,
    rows_left: i64,
    bytes_left: i64,
) -> Result<(), ForkError> {
    let points = conn.prepare("SELECT turn_id,pre_seq,pre_context,post_seq,post_context FROM conversation_points WHERE session_id=?1 AND active=1 ORDER BY pre_seq")?
        .query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<String>>(4)?)))?
        .filter_map(|row| match row { Ok(p) if turns.contains_key(&p.0) => Some(Ok(p)), Ok(_) => None, Err(e) => Some(Err(e)) })
        .collect::<Result<Vec<_>,_>>()?;
    let boundary: Option<String> = conn.query_row("SELECT p.pre_context FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id WHERE a.session_id=?1 AND a.user_message=?2 AND p.active=1",params![source,before],|r|r.get(0)).optional()?;
    let mut digests = HashSet::new();
    for p in &points {
        digests.insert(p.2.clone());
        if let Some(post) = &p.4 {
            digests.insert(post.clone());
        }
    }
    if let Some(digest) = &boundary {
        digests.insert(digest.clone());
    }
    if digests.is_empty() {
        // An actual legacy prefix has no version to import. Its rows remain
        // archival and actions report unavailable rather than inventing points.
        return Ok(());
    }
    // Distinct immutable metadata is charged once, not once per point. Check
    // with SQL lengths before fetching payloads into Rust; existing fork caps
    // also cover metadata and points. No future source context is inspected.
    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS fork_objects(id INTEGER PRIMARY KEY); DELETE FROM fork_objects;
        CREATE TEMP TABLE IF NOT EXISTS fork_restore(kind INTEGER,object_id INTEGER,row_key TEXT); DELETE FROM fork_restore;")?;
    for digest in digests {
        Db::load_context_restore(conn, source, &digest)?;
        conn.execute(
            "INSERT OR IGNORE INTO fork_objects SELECT object_id FROM conversation_restore",
            [],
        )?;
    }
    let (n,bytes): (i64,i64) = conn.query_row("SELECT count(*),coalesce(sum(length(CAST(o.payload AS BLOB))),0) FROM fork_objects f JOIN conversation_objects o ON o.id=f.id",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if n + points.len() as i64 > rows_left || bytes > bytes_left {
        return Err(refuse("copy budget exceeded").into());
    }
    let mut import = Import {
        conn,
        source,
        root,
        messages,
        turns,
        operations: operations.clone(),
        blocks: HashMap::new(),
        objects: HashMap::new(),
        contexts: HashMap::new(),
        bytes: bytes_left - bytes,
        rows: rows_left - n - points.len() as i64,
    };
    let rebase_seq = |seq: i64| seqs.range(..=seq).next_back().map_or(0, |(_, new)| *new);
    let mut last_post = None;
    for (old, pre, pre_context, post, post_context) in points {
        let pre_context = import.context(&pre_context)?;
        let post_context = post_context
            .as_deref()
            .map(|c| import.context(c))
            .transpose()?;
        conn.execute("INSERT INTO conversation_points(turn_id,session_id,pre_seq,pre_context,post_seq,post_context) VALUES (?1,?2,?3,?4,?5,?6)",params![turns[&old],root,rebase_seq(pre),pre_context,post.map(rebase_seq),post_context])?;
        last_post = post_context;
    }
    // Prefer the exact admission cut. A legacy boundary has no such version;
    // the latest genuine copied post is still causal, never today's source DCP.
    let initial = boundary
        .as_deref()
        .map(|c| import.context(c))
        .transpose()?
        .or(last_post);
    if let Some(initial) = initial {
        Db::load_context_restore(conn, root, &initial)?;
        Db::apply_context_restore(conn, root)?;
    }
    conn.execute_batch(
        "DELETE FROM fork_objects; DELETE FROM fork_restore; DELETE FROM conversation_restore;",
    )?;
    Ok(())
}
