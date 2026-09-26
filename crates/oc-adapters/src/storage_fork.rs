//! Atomic root forks. Database references are rebased; provider wire identities
//! are deliberately left paired and retain their original provenance.
use super::*;
use oc_core::{domain::SessionId, queries::ForkSessionSnapshot, session::CoreError};
use std::collections::HashMap;

#[path = "storage_fork_context.rs"]
mod context;

const MAX_ROWS: i64 = 4096;
const MAX_BYTES: i64 = 16 * 1024 * 1024;

fn refuse(reason: &str) -> CoreError {
    CoreError::Application(format!("fork refused: {reason}"))
}

fn accepted_model_for_user(
    conn: &Connection,
    source: &str,
    user: &str,
) -> Result<Option<String>, ForkError> {
    let association: Option<Option<String>> = conn.query_row(
        "SELECT CASE WHEN length(CAST(model_ref AS BLOB))<=65536 THEN model_ref END FROM turn_acceptances WHERE session_id=?1 AND user_message=?2",
        params![source,user],|r|r.get(0)).optional()?;
    let raw = match association {
        Some(raw) => Some(raw),
        None => conn.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END FROM events WHERE session_id=?1 AND kind='accepted_model' AND seq < (SELECT min(seq) FROM events WHERE session_id=?1 AND kind='message' AND payload=?2) ORDER BY seq DESC LIMIT 1",params![source,user],|r|r.get(0)).optional()?,
    };
    raw.map(|raw| raw.ok_or_else(|| refuse("accepted model budget exceeded").into()))
        .transpose()
}

impl Db {
    pub(crate) fn fork_session(
        &self,
        source: &str,
        before: &str,
        location: &str,
        provider: &str,
        choice: &str,
    ) -> Result<ForkSessionSnapshot, CoreError> {
        self.fork_transaction(source, before, location, provider, choice)
            .map_err(|error| match error {
                ForkError::Refusal(error) => error,
                ForkError::Storage => refuse("storage unavailable"),
            })
    }

    fn fork_transaction(
        &self,
        source: &str,
        before: &str,
        location: &str,
        provider: &str,
        choice: &str,
    ) -> Result<ForkSessionSnapshot, ForkError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let valid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key=?2 WHERE s.id=?1 AND s.parent_id IS NULL AND p.value=?3)",
            params![source, format!("{SESSION_LOCATION_PREFIX}{source}"), location], |r| r.get(0))?;
        if !valid {
            return Err(refuse("source is not a root in this Location").into());
        }
        let boundary: Option<(i64, String, String)> = tx.query_row(
            "SELECT seq,role,text FROM conversation_messages WHERE session_id=?1 AND id=?2 AND length(CAST(text AS BLOB))<=?3",
            params![source,before,oc_core::session::MAX_INPUT_BYTES as i64],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((mut cutoff, role, prompt)) = boundary else {
            return Err(refuse("invalid boundary").into());
        };
        if role != "user" {
            return Err(refuse("boundary is not a user message").into());
        }
        // A switch notice belongs to the selected prompt, not its preceding turn.
        if let Some((seq, role)) = tx.query_row("SELECT seq,role FROM conversation_messages WHERE session_id=?1 AND seq<?2 ORDER BY seq DESC LIMIT 1", params![source,cutoff], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional()?
            && role == "model_switch" { cutoff = seq; }
        let stored =
            match Self::get_pref_bounded_in(&tx, &tab_deck_key(location), MAX_TAB_DECK_BYTES)? {
                BoundedPref::Missing => parse_stored_deck(None)?,
                BoundedPref::Value(raw) => parse_stored_deck(Some(&raw))?,
                BoundedPref::TooLarge => return Err(refuse("invalid tab deck").into()),
            };
        let mut occupied: HashSet<String> = stored.sessions.into_iter().collect();
        occupied.extend(Self::tab_adoptions_in(&tx, location)?);
        if occupied.len() >= MAX_TABS {
            return Err(refuse("tab capacity reached").into());
        }
        // Check bytes before fetching TEXT. Includes all source turn/tool rows:
        // oversized future records safely refuse instead of unbounded allocation.
        let (rows, bytes): (i64,i64) = tx.query_row(
            "SELECT sum(n),sum(b) FROM (
              SELECT count(*) n,coalesce(sum(length(CAST(id AS BLOB))+length(CAST(role AS BLOB))+length(CAST(text AS BLOB))),0) b FROM messages WHERE session_id=?1 AND seq<?2
              UNION ALL SELECT count(*),coalesce(sum(length(CAST(id AS BLOB))+length(CAST(status AS BLOB))+length(CAST(prompt AS BLOB))+coalesce(length(CAST(result AS BLOB)),0)),0) FROM turns WHERE session_id=?1
              UNION ALL SELECT count(*),coalesce(sum(length(CAST(id AS BLOB))+coalesce(length(CAST(turn_id AS BLOB)),0)+length(CAST(state AS BLOB))+length(CAST(name AS BLOB))+coalesce(length(CAST(input AS BLOB)),0)+coalesce(length(CAST(output AS BLOB)),0)),0) FROM tool_operations WHERE session_id=?1
              UNION ALL SELECT count(*),coalesce(sum(length(CAST(turn_id AS BLOB))+length(CAST(user_message AS BLOB))+length(CAST(model_ref AS BLOB))),0) FROM turn_acceptances WHERE session_id=?1
              UNION ALL SELECT 0,coalesce(length(CAST(title AS BLOB)),0) FROM sessions WHERE id=?1)",
            params![source,cutoff], |r| Ok((r.get(0)?,r.get(1)?)))?;
        if rows > MAX_ROWS || bytes > MAX_BYTES || choice.len() as i64 > MAX_BYTES {
            return Err(refuse("copy budget exceeded").into());
        }
        let root: String =
            tx.query_row("SELECT 'fork-' || lower(hex(randomblob(16)))", [], |r| {
                r.get(0)
            })?;
        if tab_adoption_key(location, &root).len() > MAX_TAB_ADOPTION_KEY_BYTES {
            return Err(refuse("invalid Location scope").into());
        }
        let unanchored_tool: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM conversation_tools o WHERE o.session_id=?1 AND (o.turn_id IS NULL OR NOT EXISTS(SELECT 1 FROM conversation_turns t WHERE t.id=o.turn_id AND t.session_id=?1)))",[source],|r|r.get(0))?;
        if unanchored_tool {
            return Err(refuse("unanchored tool record").into());
        }
        let messages = tx
            .prepare(
                "SELECT id,role,text,seq FROM conversation_messages WHERE session_id=?1 AND seq<?2 ORDER BY seq",
            )?
            .query_map(params![source, cutoff], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let turns = tx
            .prepare(
                "SELECT t.id,t.status,t.prompt,t.result,COALESCE(a.user_message,CASE WHEN json_valid(t.result) THEN json_extract(t.result,'$.user_message') END)
                 FROM conversation_turns t LEFT JOIN turn_acceptances a ON a.turn_id=t.id AND a.session_id=t.session_id WHERE t.session_id=?1 ORDER BY t.archive_rowid",
            )?
            .query_map([source], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut prepared = Vec::new();
        let mut user_owners = HashSet::new();
        let mut assistant_owners = HashSet::new();
        for (old, status, prompt, raw, anchor) in turns {
            let anchor = anchor.ok_or_else(|| refuse("unlocated turn"))?;
            let anchor_row: Option<(i64, String)> = tx
                .query_row(
                    "SELECT seq,role FROM messages WHERE session_id=?1 AND id=?2",
                    params![source, anchor],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((anchor_seq, anchor_role)) = anchor_row else {
                return Err(refuse("foreign turn anchor").into());
            };
            if anchor_role != "user" {
                return Err(refuse("invalid turn ownership").into());
            }
            // Admission is independent of checkpoints. A recovered unknown
            // future turn cannot poison an earlier, settled fork boundary.
            if anchor_seq >= cutoff {
                continue;
            }
            if !user_owners.insert(anchor.clone()) {
                return Err(refuse("duplicate user ownership").into());
            }
            let Some(raw) = raw else {
                return Err(refuse("unanchored or unsettled turn").into());
            };
            let log: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| refuse("invalid turn log"))?;
            if log["user_message"].as_str() != Some(anchor.as_str())
                || log["turn_id"].as_str() != Some(old.as_str())
            {
                return Err(refuse("invalid turn ownership").into());
            }
            if !matches!(
                status.as_str(),
                "completed" | "failed" | "cancelled" | "interrupted"
            ) {
                return Err(refuse("unsettled prefix turn").into());
            }
            let wire =
                crate::tools::TurnLog::from_json(&log).map_err(|_| refuse("invalid wire log"))?;
            if wire.provider != provider {
                return Err(refuse("prefix belongs to another provider").into());
            }
            if let Some(assistant) = log.get("assistant_message") {
                let assistant = assistant
                    .as_str()
                    .ok_or_else(|| refuse("invalid assistant reference"))?;
                let Some((_, role, _, seq)) = messages.iter().find(|row| row.0 == assistant) else {
                    return Err(refuse("turn crosses boundary").into());
                };
                if role != "assistant"
                    || *seq <= anchor_seq
                    || messages
                        .iter()
                        .any(|row| row.1 == "user" && row.3 > anchor_seq && row.3 < *seq)
                    || !assistant_owners.insert(assistant.to_string())
                {
                    return Err(refuse("invalid assistant ownership").into());
                }
            }
            if let Some(parts) = log.get("display_parts") {
                let parts = parts
                    .as_array()
                    .ok_or_else(|| refuse("invalid display parts"))?;
                for part in parts {
                    if let Some(op) = part.get("tool") {
                        let op = op
                            .as_str()
                            .ok_or_else(|| refuse("invalid tool reference"))?;
                        let owned: bool = tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM tool_operations WHERE id=?1 AND session_id=?2 AND turn_id=?3)",
                            params![op, source, old], |r| r.get(0))?;
                        if !owned {
                            return Err(refuse("foreign tool reference").into());
                        }
                    }
                    if let Some(index) = part.get("message") {
                        let index = index
                            .as_u64()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or_else(|| refuse("invalid display message index"))?;
                        let valid = match wire.input.get(index) {
                            Some(crate::provider::InputItem::Message { role, .. }) => {
                                *role == crate::provider::InputRole::Assistant
                            }
                            Some(crate::provider::InputItem::ProviderOutput(value)) => {
                                value["type"] == "message" && value["role"] == "assistant"
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(refuse("display message is not assistant wire text").into());
                        }
                    }
                }
            }
            let mut calls = HashSet::new();
            let mut outputs = HashSet::new();
            for item in &wire.input {
                match item {
                    crate::provider::InputItem::ProviderOutput(value)
                        if value["type"] == "function_call" =>
                    {
                        let call = value["call_id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .ok_or_else(|| refuse("invalid wire call identity"))?;
                        if !calls.insert(call) {
                            return Err(refuse("duplicate wire call identity").into());
                        }
                    }
                    crate::provider::InputItem::FunctionCallOutput { call_id, .. } => {
                        if !calls.contains(call_id.as_str()) {
                            return Err(refuse("wire output precedes call").into());
                        }
                        if !outputs.insert(call_id.as_str()) {
                            return Err(refuse("duplicate wire output identity").into());
                        }
                    }
                    _ => {}
                }
            }
            if calls != outputs {
                return Err(refuse("unpaired wire tool context").into());
            }
            prepared.push((old, status, prompt, anchor, log));
        }
        let mut models = HashMap::new();
        let mut copied_bytes = bytes;
        for (old, role, _, _) in &messages {
            if role != "user" {
                continue;
            }
            if let Some(raw) = accepted_model_for_user(&tx, source, old)? {
                // Fallback journal references may be shared by many unassociated
                // users. Charge each copy before parsing or retaining it, even
                // when its source acceptance row was already budgeted above.
                copied_bytes = copied_bytes.saturating_add(raw.len() as i64);
                if copied_bytes > MAX_BYTES {
                    return Err(refuse("copy budget exceeded").into());
                }
                let model: oc_core::queries::ModelRef =
                    serde_json::from_str(&raw).map_err(|_| refuse("invalid accepted model"))?;
                if model.provider != provider {
                    return Err(refuse("accepted model belongs to another provider").into());
                }
                if let Some(turn) = prepared.iter().find(|turn| &turn.3 == old)
                    && (turn.4["model"] != model.id || turn.4["provider"] != model.provider)
                {
                    return Err(refuse("wire provenance differs from acceptance").into());
                }
                models.insert(old.clone(), raw);
            } else if prepared.iter().any(|turn| &turn.3 == old) {
                return Err(refuse("missing accepted model association").into());
            }
        }
        Self::insert_root_session(&tx, &root)?;
        Self::insert_location_binding(&tx, &root, location)?;
        tx.execute("UPDATE sessions SET title=(SELECT CASE WHEN title IS NULL THEN NULL ELSE title || ' (fork)' END FROM sessions WHERE id=?2) WHERE id=?1",params![root,source])?;
        let mut ids = HashMap::new();
        let mut seqs = std::collections::BTreeMap::new();
        for (old, role, text, seq) in messages {
            if role == "user" {
                // Recreate the admission event before each copied user, not a
                // single terminal event after the whole prefix. This remains
                // correct when the destination is forked again at any boundary.
                if let Some(raw) = models.get(&old) {
                    tx.execute("INSERT INTO events(session_id,kind,payload) VALUES (?1,'accepted_model',?2)",params![root,raw])?;
                }
            }
            let new_id = Self::insert_message(&tx, &root, &role, &text)?;
            let new_seq: i64 =
                tx.query_row("SELECT seq FROM messages WHERE id=?1", [&new_id], |r| {
                    r.get(0)
                })?;
            seqs.insert(seq, new_seq);
            ids.insert(old, new_id);
        }
        let mut turn_ids = HashMap::new();
        for (old, status, prompt, anchor, mut log) in prepared {
            let model = models
                .get(&anchor)
                .ok_or_else(|| refuse("missing accepted model association"))?;
            let anchor = ids
                .get(&anchor)
                .ok_or_else(|| refuse("missing copied anchor"))?;
            let new = format!("{root}:turn:{}", turn_ids.len());
            log["turn_id"] = new.clone().into();
            log["user_message"] = anchor.clone().into();
            if let Some(assistant) = log["assistant_message"].as_str() {
                log["assistant_message"] = ids
                    .get(assistant)
                    .ok_or_else(|| refuse("turn crosses boundary"))?
                    .clone()
                    .into();
            }
            let ops = tx.prepare("SELECT id,name,state,input,output FROM tool_operations WHERE session_id=?1 AND turn_id=?2 ORDER BY rowid")?
                .query_map(params![source,old], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?)))?
                .collect::<Result<Vec<_>,_>>()?;
            let mut op_ids = HashMap::new();
            for (op, name, state, input, output) in ops {
                if !matches!(
                    state.as_str(),
                    "completed" | "failed" | "denied" | "cancelled" | "no_gain"
                ) {
                    return Err(refuse("unresolved tool outcome").into());
                }
                let new_op = format!("{new}:op:{}", op_ids.len());
                tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![new_op,root,new,name,state,input,output])?;
                op_ids.insert(op, new_op);
            }
            if let Some(parts) = log["display_parts"].as_array_mut() {
                for part in parts {
                    if let Some(op) = part.get("tool") {
                        let op = op
                            .as_str()
                            .ok_or_else(|| refuse("invalid tool reference"))?;
                        part["tool"] = op_ids
                            .get(op)
                            .ok_or_else(|| refuse("foreign tool reference"))?
                            .clone()
                            .into();
                    }
                }
            }
            tx.execute(
                "INSERT INTO turns(id,session_id,status,prompt,result) VALUES (?1,?2,?3,?4,?5)",
                params![new, root, status, prompt, log.to_string()],
            )?;
            tx.execute("INSERT INTO turn_acceptances(turn_id,session_id,user_message,model_ref) VALUES (?1,?2,?3,?4)",params![new,root,anchor,model])?;
            turn_ids.insert(old, new);
        }
        context::copy(
            &tx,
            source,
            before,
            &root,
            &ids,
            &turn_ids,
            &seqs,
            MAX_ROWS - rows,
            MAX_BYTES - copied_bytes,
        )?;
        let selection_key = format!(
            "tui.selection.session:{}",
            serde_json::to_string(&[location, provider, &root]).expect("strings")
        );
        Self::upsert_pref(&tx, &selection_key, choice)?;
        Self::upsert_pref(&tx, &tab_adoption_key(location, &root), TAB_ADOPTION_VALUE)?;
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES (?1,'session_forked',?2)",
            params![
                root,
                serde_json::json!({"source":source,"before":before}).to_string()
            ],
        )?;
        tx.commit()?;
        Ok(ForkSessionSnapshot {
            session: SessionId(root),
            prompt,
        })
    }
}

enum ForkError {
    Refusal(CoreError),
    Storage,
}
impl From<CoreError> for ForkError {
    fn from(e: CoreError) -> Self {
        Self::Refusal(e)
    }
}
impl From<StorageError> for ForkError {
    fn from(_: StorageError) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for ForkError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::TurnLog;

    #[test]
    fn fork_saved_context_points_rebase_restart_recursive_and_inactive_branches() {
        use oc_core::queries::ConversationAction::{Redo, Revert, Undo};
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_bound_session("source", "/project").unwrap();
        db.apply_dcp_schema().unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let first = db
            .accept_turn("first", "source", "first", "first", &model)
            .unwrap()
            .user_message;
        let block = db
            .save_compression_block(
                "source",
                "first",
                "FIRST SAVED POST",
                &first,
                &first,
                std::slice::from_ref(&first),
            )
            .unwrap();
        db.save_prune_mark("source", &first).unwrap();
        db.set_pref(
            "dcp.nudge.source\0fixture\0m",
            "{\"turns_since_compress\":1}",
        )
        .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO dcp_tool_projection_v2 VALUES ('source','wire-call',0,'hidden')",
                [],
            )
            .unwrap();
        let mut log = TurnLog::new("first", "m", "fixture");
        log.user_message = Some(first.clone());
        log.input = vec![
            crate::provider::InputItem::message(crate::provider::InputRole::User, "first"),
            crate::provider::InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","call_id":"wire-call","id":"wire-item","name":"bash","arguments":"{}"}),
            ),
            crate::provider::InputItem::FunctionCallOutput {
                call_id: "wire-call".into(),
                output: "saved output".into(),
            },
        ];
        log.display_parts = vec![serde_json::json!({"tool":"wire-call"})];
        // A provider identity may happen to equal the local op primary key.
        // Only the latter is rebased; hide/purge must still match wire input.
        db.record_turn_tool_intent(
            "wire-call",
            "source",
            "first",
            "bash",
            "{}",
            &log.to_json().to_string(),
        )
        .unwrap();
        db.record_tool_outcome("wire-call", "completed", Some("saved output"))
            .unwrap();
        db.commit_turn(
            "first",
            "completed",
            Some(&log.to_json().to_string()),
            Some("first answer"),
        )
        .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE compression_blocks SET summary='EXACT ADMISSION CUT' WHERE id=?1",
                [&block],
            )
            .unwrap();
        let second = complete(&db, "source", "second", &model);
        let third = complete(&db, "source", "third", &model);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE compression_blocks SET summary='FUTURE SOURCE LEAK' WHERE id=?1",
                [&block],
            )
            .unwrap();
        let archive = db.read_history_full("source").unwrap();
        let original_blocks = db.load_compression_blocks("source").unwrap();
        let fork = db
            .fork_session("source", &second, "/project", "fixture", "{}")
            .unwrap();
        let root = &fork.session.0;
        let copied = db.read_history_full(root).unwrap();
        assert_eq!(copied.len(), 2);
        let historical = db.load_compression_blocks(root).unwrap();
        assert_eq!(historical[0].summary, "EXACT ADMISSION CUT");
        assert_ne!(historical[0].id, block);
        assert_eq!(historical[0].start_msg, copied[0].0);
        assert_eq!(historical[0].end_msg, copied[0].0);
        assert_eq!(historical[0].members, [copied[0].0.clone()]);
        assert_eq!(db.load_prune_mark(root).unwrap(), Some(copied[0].0.clone()));
        assert!(
            db.load_dcp_tool_projection(root)
                .unwrap()
                .hidden
                .contains(&("wire-call".into(), 0))
        );
        assert_ne!(db.list_tool_ops(root).unwrap()[0].op, "wire-call");
        assert_eq!(
            db.conversation_nudges(root).unwrap()[0].0,
            format!("dcp.nudge.{root}\0fixture\0m")
        );
        let undone = db
            .change_conversation(
                root,
                Revert {
                    message: oc_core::session::MessageId(copied[0].0.clone()),
                },
            )
            .unwrap();
        assert_eq!(undone.draft.as_deref(), Some("first"));
        assert!(!undone.can_undo && undone.can_redo);
        assert!(db.conversation_history_full(root).unwrap().is_empty());
        assert!(db.load_compression_blocks(root).unwrap().is_empty());
        assert!(db.conversation_nudges(root).unwrap().is_empty());
        assert_eq!(db.read_history_full("source").unwrap(), archive);
        assert_eq!(
            db.load_compression_blocks("source").unwrap()[0].summary,
            original_blocks[0].summary
        );
        drop(db);
        let db = Db::open(tmp.path()).unwrap();
        db.change_conversation(root, Redo).unwrap();
        assert_eq!(
            db.load_compression_blocks(root).unwrap()[0].summary,
            "FIRST SAVED POST"
        );
        db.change_conversation(root, Undo).unwrap();
        db.change_conversation(root, Redo).unwrap();
        let two = db
            .fork_session("source", &third, "/project", "fixture", "{}")
            .unwrap();
        let rows = db.read_history_full(&two.session.0).unwrap();
        let nested = db
            .fork_session(&two.session.0, &rows[2].0, "/project", "fixture", "{}")
            .unwrap();
        assert_eq!(
            db.load_compression_blocks(&nested.session.0).unwrap()[0].summary,
            "EXACT ADMISSION CUT"
        );
        db.change_conversation(&nested.session.0, Undo).unwrap();
        db.change_conversation(&nested.session.0, Redo).unwrap();
        assert_eq!(
            db.load_compression_blocks(&nested.session.0).unwrap()[0].summary,
            "FIRST SAVED POST"
        );
        // A replacement branch must not import the old second/third points.
        db.change_conversation("source", Undo).unwrap();
        db.change_conversation("source", Undo).unwrap();
        let replacement = complete(&db, "source", "replacement", &model);
        let branch = db
            .fork_session("source", &replacement, "/project", "fixture", "{}")
            .unwrap();
        assert_eq!(db.read_history_full(&branch.session.0).unwrap().len(), 2);
        db.change_conversation(&branch.session.0, Undo).unwrap();
        let restored = db.change_conversation(&branch.session.0, Redo).unwrap();
        assert!(!restored.can_redo);
        assert_eq!(
            db.load_compression_blocks(&branch.session.0).unwrap()[0].summary,
            "FIRST SAVED POST"
        );
        assert_eq!(
            db.read_history_full("source").unwrap()[..archive.len()],
            archive
        );
        // Imported point failure rolls back root/deck/metadata and block IDs.
        let conn = db.conn.lock().unwrap();
        conn.execute_batch("CREATE TEMP TRIGGER fail_fork_point BEFORE INSERT ON conversation_points WHEN NEW.session_id LIKE 'fork-%' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        let before_counts: (i64,i64,i64,i64) = conn.query_row("SELECT (SELECT count(*) FROM conversation_versions),(SELECT count(*) FROM conversation_objects),(SELECT count(*) FROM conversation_points),(SELECT high_water FROM compression_identity)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        drop(conn);
        let before = counts(&db);
        assert!(
            db.fork_session("source", &replacement, "/project", "fixture", "{}")
                .is_err()
        );
        assert_eq!(counts(&db), before);
        let after_counts = db.conn.lock().unwrap().query_row("SELECT (SELECT count(*) FROM conversation_versions),(SELECT count(*) FROM conversation_objects),(SELECT count(*) FROM conversation_points),(SELECT high_water FROM compression_identity)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).unwrap();
        assert_eq!(before_counts, after_counts);
    }

    #[test]
    fn fork_genuinely_legacy_prefix_has_no_fabricated_points_or_future_dcp() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_bound_session("source", "/project").unwrap();
        db.apply_dcp_schema().unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let first = complete(&db, "source", "legacy-first", &model);
        let second = complete(&db, "source", "legacy-second", &model);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM conversation_points WHERE session_id='source'",
                [],
            )
            .unwrap();
        db.save_compression_block(
            "source",
            "future",
            "not a real historical version",
            &second,
            &second,
            std::slice::from_ref(&second),
        )
        .unwrap();
        let fork = db
            .fork_session("source", &second, "/project", "fixture", "{}")
            .unwrap();
        let rows = db.read_history_full(&fork.session.0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].0, first);
        assert!(
            db.load_compression_blocks(&fork.session.0)
                .unwrap()
                .is_empty()
        );
        for action in [
            oc_core::queries::ConversationAction::Undo,
            oc_core::queries::ConversationAction::Revert {
                message: oc_core::session::MessageId(rows[0].0.clone()),
            },
        ] {
            let error = db
                .change_conversation(&fork.session.0, action)
                .unwrap_err()
                .to_string();
            assert!(error.contains("no saved historical context"), "{error}");
        }
        let empty = db
            .fork_session("source", &first, "/project", "fixture", "{}")
            .unwrap();
        assert!(
            db.conversation_history_full(&empty.session.0)
                .unwrap()
                .is_empty()
        );
        assert!(
            db.change_conversation(&empty.session.0, oc_core::queries::ConversationAction::Undo)
                .is_err()
        );
        db.create_bound_session("mixed", "/project").unwrap();
        complete(&db, "mixed", "supported-one", &model);
        complete(&db, "mixed", "missing-two", &model);
        let third = complete(&db, "mixed", "supported-three", &model);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM conversation_points WHERE turn_id='missing-two'",
                [],
            )
            .unwrap();
        let mixed = db
            .fork_session("mixed", &third, "/project", "fixture", "{}")
            .unwrap();
        let rows = db.read_history_full(&mixed.session.0).unwrap();
        // Do not skip the nearest legacy turn and undo some earlier point.
        assert!(
            db.change_conversation(&mixed.session.0, oc_core::queries::ConversationAction::Undo)
                .is_err()
        );
        db.change_conversation(
            &mixed.session.0,
            oc_core::queries::ConversationAction::Revert {
                message: oc_core::session::MessageId(rows[0].0.clone()),
            },
        )
        .unwrap();
        // Whole-tail Redo cannot substitute an earlier supported post point
        // for this tip's missing historical revision.
        assert!(
            db.change_conversation(&mixed.session.0, oc_core::queries::ConversationAction::Redo)
                .is_err()
        );
        assert_eq!(
            db.conversation_history_full(&mixed.session.0)
                .unwrap()
                .len(),
            0
        );
        assert!(
            db.change_conversation(&mixed.session.0, oc_core::queries::ConversationAction::Redo)
                .is_err()
        );
    }

    fn complete(db: &Db, session: &str, turn: &str, model: &oc_core::queries::ModelRef) -> String {
        let user = db
            .accept_turn(turn, session, "prompt", "prompt", model)
            .unwrap()
            .user_message;
        let mut log = TurnLog::new(turn, &model.id, &model.provider);
        log.user_message = Some(user.clone());
        log.input = vec![
            crate::provider::InputItem::message(crate::provider::InputRole::User, "prompt"),
            crate::provider::InputItem::message(crate::provider::InputRole::Assistant, "answer"),
        ];
        log.display_parts = vec![serde_json::json!({"message":1})];
        db.commit_turn(
            turn,
            "completed",
            Some(&log.to_json().to_string()),
            Some("answer"),
        )
        .unwrap();
        user
    }

    fn counts(db: &Db) -> Vec<i64> {
        let conn = db.conn.lock().unwrap();
        [
            "sessions",
            "messages",
            "turns",
            "turn_acceptances",
            "tool_operations",
            "events",
            "prefs",
        ]
        .iter()
        .map(|table| {
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .collect()
    }

    #[test]
    fn recursive_forks_preserve_every_copied_users_model_acceptance() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_bound_session("source", "/project").unwrap();
        let mut model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "old".into(),
            variant: Some("deep".into()),
        };
        complete(&db, "source", "first", &model);
        model.id = "new".into();
        model.variant = None;
        complete(&db, "source", "second", &model);
        let boundary = complete(&db, "source", "third", &model);
        let fork = db
            .fork_session("source", &boundary, "/project", "fixture", "{}")
            .unwrap();
        let copied = db.read_history_full(&fork.session.0).unwrap();
        let recursive = db
            .fork_session(&fork.session.0, &copied[2].0, "/project", "fixture", "{}")
            .unwrap();
        model.id = "third-model".into();
        let accepted = db
            .accept_turn("after-recursive", &recursive.session.0, "p", "p", &model)
            .unwrap();
        let notice = accepted.model_switch.unwrap();
        assert_eq!(notice.previous.id, "old");
        assert_eq!(notice.previous.variant.as_deref(), Some("deep"));
        // A first newly accepted user in the original fork must see the newest
        // copied admission; forking before that new user must retain it too.
        let new_user = complete(&db, &fork.session.0, "new-turn", &model);
        let recursive = db
            .fork_session(&fork.session.0, &new_user, "/project", "fixture", "{}")
            .unwrap();
        let accepted = db
            .accept_turn("after-new-boundary", &recursive.session.0, "p", "p", &model)
            .unwrap();
        assert_eq!(accepted.model_switch.unwrap().previous.id, "new");
        let conn = db.conn.lock().unwrap();
        let ordered: bool = conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM turn_acceptances a JOIN events u ON u.session_id=a.session_id AND u.kind='message' AND u.payload=a.user_message WHERE a.session_id=?1 AND NOT EXISTS(SELECT 1 FROM events e WHERE e.session_id=a.session_id AND e.kind='accepted_model' AND e.payload=a.model_ref AND e.seq<u.seq))",[&fork.session.0],|r|r.get(0)).unwrap();
        assert!(ordered);
    }

    #[test]
    fn future_uncheckpointed_recovered_unknown_does_not_block_prefix_or_empty_fork() {
        let tmp = tempfile::tempdir().unwrap();
        let user;
        let boundary;
        {
            let db = Db::open(tmp.path()).unwrap();
            (user, boundary) = seed(&db);
            let model = oc_core::queries::ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            };
            db.accept_turn("future-null", "source", "future", "future", &model)
                .unwrap();
            db.recover_interrupted_tools().unwrap();
            assert_eq!(
                db.turn_result("future-null").unwrap(),
                ("unknown".into(), None)
            );
            // Exercise upgrade from admission events, rather than relying only
            // on the new acceptance table created by this process.
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "DELETE FROM turn_acceptances WHERE turn_id='future-null'",
                    [],
                )
                .unwrap();
            db.conn
                .lock()
                .unwrap()
                .execute("DELETE FROM schema_migrations WHERE version=4", [])
                .unwrap();
        }
        let db = Db::open(tmp.path()).unwrap();
        let fork = db
            .fork_session("source", &boundary, "/project", "fixture", "{}")
            .unwrap();
        assert_eq!(db.read_history_full(&fork.session.0).unwrap().len(), 2);
        let empty = db
            .fork_session("source", &user, "/project", "fixture", "{}")
            .unwrap();
        assert!(db.read_history_full(&empty.session.0).unwrap().is_empty());
        let after_unknown = db
            .append_message("source", "user", "after unknown")
            .unwrap();
        let before = counts(&db);
        assert!(
            db.fork_session("source", &after_unknown, "/project", "fixture", "{}")
                .is_err()
        );
        assert_eq!(counts(&db), before);
    }

    #[test]
    fn invalid_assistant_and_display_references_refuse_without_any_partial_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        let (user, _) = seed(&db);
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let other_user = complete(&db, "source", "second-turn", &model);
        let other_assistant = db
            .read_history_full("source")
            .unwrap()
            .last()
            .unwrap()
            .0
            .clone();
        let boundary = db.append_message("source", "user", "end").unwrap();
        let original = db.turn_result("original-turn").unwrap().1.unwrap();
        let baseline = counts(&db);
        let mut mutations = Vec::new();
        for target in [user, other_user, other_assistant] {
            let mut log: serde_json::Value = serde_json::from_str(&original).unwrap();
            log["assistant_message"] = target.into();
            mutations.push(log);
        }
        for index in [
            serde_json::json!(null),
            serde_json::json!("3"),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!(999),
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(2),
        ] {
            let mut log: serde_json::Value = serde_json::from_str(&original).unwrap();
            log["display_parts"][1]["message"] = index;
            mutations.push(log);
        }
        for log in mutations {
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE turns SET result=?1 WHERE id='original-turn'",
                    [log.to_string()],
                )
                .unwrap();
            assert!(
                db.fork_session("source", &boundary, "/project", "fixture", "{}")
                    .is_err()
            );
            assert_eq!(counts(&db), baseline);
            assert!(db.tab_adoptions("/project").unwrap().is_empty());
        }
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE turns SET result=?1 WHERE id='original-turn'",
                [&original],
            )
            .unwrap();
        // A separately journaled turn cannot take ownership of the same user
        // and assistant, even when each individual reference is well typed.
        let mut alias: serde_json::Value = serde_json::from_str(&original).unwrap();
        alias["turn_id"] = "alias-turn".into();
        db.begin_turn("alias-turn", "source", "first").unwrap();
        db.finish_turn("alias-turn", "completed", Some(&alias.to_string()))
            .unwrap();
        let with_alias = counts(&db);
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
        assert_eq!(counts(&db), with_alias);
        db.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM turns WHERE id='alias-turn'", [])
            .unwrap();
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_ok()
        );
    }

    #[test]
    fn foreign_provider_is_refused_before_root_insert_and_reopen_keeps_source_only() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let db = Db::open(tmp.path()).unwrap();
            let (_, boundary) = seed(&db);
            let baseline = counts(&db);
            // The error must be provenance refusal, not this insertion trigger.
            db.conn.lock().unwrap().execute_batch("CREATE TRIGGER no_new_root BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'root insertion reached'); END;").unwrap();
            let error = db
                .fork_session("source", &boundary, "/project", "foreign", "{}")
                .unwrap_err();
            assert!(matches!(error,CoreError::Application(reason) if reason.contains("provider")));
            assert_eq!(counts(&db), baseline);
        }
        let db = Db::open(tmp.path()).unwrap();
        assert_eq!(db.list_sessions().unwrap(), ["source"]);
        assert!(db.tab_adoptions("/project").unwrap().is_empty());
    }

    #[test]
    fn repeated_unassociated_model_fallback_is_charged_before_any_root_insert() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let db = Db::open(tmp.path()).unwrap();
            db.create_bound_session("source", "/project").unwrap();
            let model = oc_core::queries::ModelRef {
                provider: "fixture".into(),
                id: "m".repeat(60_000),
                variant: None,
            };
            let raw = serde_json::to_string(&model).unwrap();
            let boundary;
            {
                let mut conn = db.conn.lock().unwrap();
                let tx = conn.transaction().unwrap();
                tx.execute("INSERT INTO events(session_id,kind,payload) VALUES ('source','accepted_model',?1)",[&raw]).unwrap();
                for _ in 0..300 {
                    Db::insert_message(&tx, "source", "user", "unassociated legacy prompt")
                        .unwrap();
                }
                boundary = Db::insert_message(&tx, "source", "user", "selected").unwrap();
                tx.commit().unwrap();
                conn.execute_batch("CREATE TRIGGER no_new_root BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'root insertion reached'); END;").unwrap();
            }
            let baseline = counts(&db);
            let history = db.read_history_full("source").unwrap();
            let error = db
                .fork_session("source", &boundary, "/project", "fixture", "{}")
                .unwrap_err();
            assert!(
                matches!(error, CoreError::Application(reason) if reason == "fork refused: copy budget exceeded")
            );
            assert_eq!(counts(&db), baseline);
            assert_eq!(db.read_history_full("source").unwrap(), history);
            assert!(db.tab_adoptions("/project").unwrap().is_empty());
        }
        let db = Db::open(tmp.path()).unwrap();
        assert_eq!(db.list_sessions().unwrap(), ["source"]);
        assert_eq!(db.read_history_full("source").unwrap().len(), 301);
        assert!(db.tab_adoptions("/project").unwrap().is_empty());
    }

    #[test]
    fn present_display_tool_must_be_string_and_owned_by_its_copied_turn() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        seed(&db);
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let other_user = db
            .accept_turn(
                "other-turn",
                "source",
                "other prompt",
                "other prompt",
                &model,
            )
            .unwrap()
            .user_message;
        db.record_turn_tool_intent(
            "other-turn-op",
            "source",
            "other-turn",
            "bash",
            "{\"command\":\"true\"}",
            "{}",
        )
        .unwrap();
        db.record_tool_outcome("other-turn-op", "completed", Some("done"))
            .unwrap();
        let mut other_log = TurnLog::new("other-turn", &model.id, &model.provider);
        other_log.user_message = Some(other_user);
        other_log.input = vec![
            crate::provider::InputItem::message(crate::provider::InputRole::User, "other prompt"),
            crate::provider::InputItem::ProviderOutput(serde_json::json!({
                "type":"function_call", "id":"other-item", "call_id":"other-call",
                "name":"bash", "arguments":"{\"command\":\"true\"}"
            })),
            crate::provider::InputItem::FunctionCallOutput {
                call_id: "other-call".into(),
                output: "done".into(),
            },
            crate::provider::InputItem::message(
                crate::provider::InputRole::Assistant,
                "other answer",
            ),
        ];
        other_log.display_parts = vec![
            serde_json::json!({"tool":"other-turn-op"}),
            serde_json::json!({"message":3}),
        ];
        db.commit_turn(
            "other-turn",
            "completed",
            Some(&other_log.to_json().to_string()),
            Some("other answer"),
        )
        .unwrap();
        db.create_session("foreign").unwrap();
        db.begin_turn("foreign-turn", "foreign", "p").unwrap();
        db.record_turn_tool_intent("foreign-op", "foreign", "foreign-turn", "bash", "{}", "{}")
            .unwrap();
        db.record_tool_outcome("foreign-op", "completed", Some("done"))
            .unwrap();
        let boundary = db.append_message("source", "user", "selected").unwrap();
        let original = db.turn_result("original-turn").unwrap().1.unwrap();
        let baseline = counts(&db);
        let history = db.read_history_full("source").unwrap();
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER no_new_root BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'root insertion reached'); END;").unwrap();
        for tool in [
            serde_json::json!(null),
            serde_json::json!(17),
            serde_json::json!("other-turn-op"),
            serde_json::json!("foreign-op"),
        ] {
            let mut log: serde_json::Value = serde_json::from_str(&original).unwrap();
            log["display_parts"][0]["tool"] = tool;
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE turns SET result=?1 WHERE id='original-turn'",
                    [log.to_string()],
                )
                .unwrap();
            let error = db
                .fork_session("source", &boundary, "/project", "fixture", "{}")
                .unwrap_err();
            assert!(
                matches!(error, CoreError::Application(reason) if reason.contains("tool reference"))
            );
            assert_eq!(counts(&db), baseline);
            assert_eq!(db.read_history_full("source").unwrap(), history);
            assert!(db.tab_adoptions("/project").unwrap().is_empty());
        }
    }

    fn seed(db: &Db) -> (String, String) {
        db.create_bound_session("source", "/project").unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let user = db
            .accept_turn("original-turn", "source", "first", "first", &model)
            .unwrap()
            .user_message;
        db.record_turn_tool_intent(
            "original-op",
            "source",
            "original-turn",
            "bash",
            "{\"command\":\"true\"}",
            "{}",
        )
        .unwrap();
        db.record_tool_outcome("original-op", "completed", Some("durable output"))
            .unwrap();
        let mut log = TurnLog::new("original-turn", "m", "fixture");
        log.user_message = Some(user.clone());
        log.input = vec![
            crate::provider::InputItem::message(crate::provider::InputRole::User, "first"),
            crate::provider::InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","id":"vendor-item","call_id":"vendor-call","name":"bash","arguments":"{\"command\":\"true\"}"}),
            ),
            crate::provider::InputItem::FunctionCallOutput {
                call_id: "vendor-call".into(),
                output: "durable output".into(),
            },
            crate::provider::InputItem::message(crate::provider::InputRole::Assistant, "done"),
        ];
        log.opaque = vec![serde_json::json!({"encrypted_content":"same-provider-only"})];
        log.display_parts = vec![
            serde_json::json!({"tool":"original-op"}),
            serde_json::json!({"message":3}),
        ];
        db.commit_turn(
            "original-turn",
            "completed",
            Some(&log.to_json().to_string()),
            Some("done"),
        )
        .unwrap();
        let boundary = db
            .append_message("source", "user", "selected draft")
            .unwrap();
        db.append_message("source", "assistant", "future").unwrap();
        (user, boundary)
    }

    #[test]
    fn fork_rebases_settled_tool_graph_reopens_and_preserves_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root;
        let before;
        {
            let db = Db::open(tmp.path()).unwrap();
            let (user, boundary) = seed(&db);
            db.apply_dcp_schema().unwrap();
            db.save_prune_mark("source", &user).unwrap();
            before = db.read_history_full("source").unwrap();
            let fork = db
                .fork_session("source", &boundary, "/project", "fixture", "{}")
                .unwrap();
            root = fork.session.0;
            assert_eq!(fork.prompt, "selected draft");
            assert_eq!(db.read_history_full("source").unwrap(), before);
            assert_eq!(db.session_meta(&root).unwrap().parent_id, None);
            assert!(db.load_prune_mark(&root).unwrap().is_none());
            assert!(db.load_compression_blocks(&root).unwrap().is_empty());
            let copied = db.read_history_full(&root).unwrap();
            assert_eq!(
                copied.iter().map(|r| r.2.as_str()).collect::<Vec<_>>(),
                ["first", "done"]
            );
            assert!(copied.iter().all(|r| before.iter().all(|old| old.0 != r.0)));
            let logs = db.wire_logs_for_window(&root, 0, 100).unwrap();
            assert_eq!(logs.len(), 1);
            let value: serde_json::Value = serde_json::from_str(&logs[0].0).unwrap();
            let log = TurnLog::from_json(&value).unwrap();
            assert_eq!(log.user_message.as_deref(), Some(copied[0].0.as_str()));
            assert_ne!(log.turn_id, "original-turn");
            assert_eq!(value["assistant_message"], copied[1].0);
            let ops = db.list_tool_ops(&root).unwrap();
            assert_eq!(ops.len(), 1);
            assert_ne!(ops[0].op, "original-op");
            assert_eq!(value["display_parts"][0]["tool"], ops[0].op);
            assert_eq!(
                serde_json::to_value(&log.input).unwrap()[1]["call_id"],
                "vendor-call"
            );
            assert_eq!(
                serde_json::to_value(&log.input).unwrap()[2]["call_id"],
                "vendor-call"
            );
            assert_eq!(
                db.read_session_tool_output(&root, &ops[0].op, 0, 100)
                    .unwrap()
                    .0,
                "durable output"
            );
            assert_eq!(db.tab_adoptions("/project").unwrap(), vec![root.clone()]);
            assert!(
                !db.compare_set_tab_deck(&tab_deck_key("/project"), None, "{}", "/project", &[])
                    .unwrap()
            );
            assert!(
                db.compare_set_tab_deck(
                    &tab_deck_key("/project"),
                    None,
                    "{}",
                    "/project",
                    std::slice::from_ref(&root)
                )
                .unwrap()
            );
        }
        let db = Db::open(tmp.path()).unwrap();
        assert_eq!(db.read_history_full("source").unwrap(), before);
        assert_eq!(db.read_history_full(&root).unwrap().len(), 2);
        assert_eq!(db.wire_logs_for_window(&root, 0, 100).unwrap().len(), 1);
        assert!(db.tab_adoptions("/project").unwrap().is_empty());
    }

    #[test]
    fn fork_refuses_invalid_foreign_unknown_oversized_and_rolls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        let (user, boundary) = seed(&db);
        let original = db.list_sessions().unwrap();
        db.create_child_session("source", "child", None, None, None)
            .unwrap();
        db.set_pref(&format!("{SESSION_LOCATION_PREFIX}child"), "/project")
            .unwrap();
        assert!(
            db.fork_session("child", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
        let foreign_boundary = db.append_message("child", "user", "foreign").unwrap();
        assert!(
            db.fork_session("source", &foreign_boundary, "/project", "fixture", "{}")
                .is_err()
        );
        let original = original
            .into_iter()
            .chain(["child".to_string()])
            .collect::<HashSet<_>>();
        for (source, before, location) in [
            ("missing", boundary.as_str(), "/project"),
            ("source", "missing", "/project"),
            ("source", boundary.as_str(), "/foreign"),
        ] {
            assert!(
                db.fork_session(source, before, location, "fixture", "{}")
                    .is_err()
            );
        }
        let assistant = db.read_history_full("source").unwrap()[1].0.clone();
        assert!(
            db.fork_session("source", &assistant, "/project", "fixture", "{}")
                .is_err()
        );
        // Legacy/corrupt archive fixtures bypass the production outcome API:
        // settled turns now reject late outcomes instead of rewriting history.
        assert!(
            db.record_tool_outcome("original-op", "unknown", Some("unknown"))
                .is_err()
        );
        db.conn.lock().unwrap().execute("UPDATE tool_operations SET state='unknown',output='unknown' WHERE id='original-op'",[]).unwrap();
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE tool_operations SET state='completed',output='ok' WHERE id='original-op'",
                [],
            )
            .unwrap();
        let before_failure = counts(&db);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch("CREATE TRIGGER fail_fork BEFORE INSERT ON events WHEN NEW.kind='session_forked' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        }
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
        assert_eq!(counts(&db), before_failure);
        assert_eq!(
            db.list_sessions()
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>(),
            original
        );
        assert!(db.tab_adoptions("/project").unwrap().is_empty());
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_fork")
            .unwrap();
        let empty = db
            .fork_session("source", &user, "/project", "fixture", "{}")
            .unwrap();
        assert_eq!(empty.prompt, "first");
        assert!(db.read_history_full(&empty.session.0).unwrap().is_empty());
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE tool_operations SET output=?1 WHERE id='original-op'",
                ["x".repeat(MAX_BYTES as usize + 1)],
            )
            .unwrap();
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
    }

    #[test]
    fn fork_capacity_and_row_budget_refusals_leave_no_root_or_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        let (_, boundary) = seed(&db);
        let deck = StoredDeck {
            version: 1,
            sessions: (0..MAX_TABS).map(|n| format!("tab-{n}")).collect(),
            active: None,
        };
        db.set_pref(
            &tab_deck_key("/project"),
            &serde_json::to_string(&deck).unwrap(),
        )
        .unwrap();
        assert!(
            db.fork_session("source", &boundary, "/project", "fixture", "{}")
                .is_err()
        );
        db.set_pref(
            &tab_deck_key("/project"),
            "{\"version\":1,\"sessions\":[],\"active\":null}",
        )
        .unwrap();
        db.conn.lock().unwrap().execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<4097) INSERT INTO messages(id,session_id,seq,role,text) SELECT 'extra-' || x,'source',100+x,'user','x' FROM n;").unwrap();
        let last = db.append_message("source", "user", "boundary").unwrap();
        assert!(
            db.fork_session("source", &last, "/project", "fixture", "{}")
                .is_err()
        );
        assert_eq!(db.list_sessions().unwrap(), ["source"]);
        assert!(db.tab_adoptions("/project").unwrap().is_empty());
    }

    #[test]
    fn fork_excludes_boundary_model_switch_and_keeps_prefix_accepted_model() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_bound_session("source", "/project").unwrap();
        let mut model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "old".into(),
            variant: None,
        };
        let mut boundary = String::new();
        for n in 0..2 {
            let turn = format!("turn-{n}");
            let user = db
                .accept_turn(&turn, "source", "prompt", "prompt", &model)
                .unwrap()
                .user_message;
            let mut log = TurnLog::new(&turn, &model.id, "fixture");
            log.user_message = Some(user.clone());
            log.input = vec![crate::provider::InputItem::message(
                crate::provider::InputRole::Assistant,
                "answer",
            )];
            db.commit_turn(
                &turn,
                "completed",
                Some(&log.to_json().to_string()),
                Some("answer"),
            )
            .unwrap();
            boundary = user;
            model.id = "new".into();
        }
        let fork = db
            .fork_session("source", &boundary, "/project", "fixture", "{}")
            .unwrap();
        let rows = db
            .read_history_page_typed(&fork.session.0, 100, None)
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.role != "model_switch"));
        let accepted: String = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM events WHERE session_id=?1 AND kind='accepted_model'",
                [&fork.session.0],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<oc_core::queries::ModelRef>(&accepted)
                .unwrap()
                .id,
            "old"
        );
        assert_eq!(
            db.wire_logs_for_window(&fork.session.0, 0, 100)
                .unwrap()
                .len(),
            1
        );
    }
}
