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
    blocks: HashMap<String, String>,
    objects: HashMap<i64, (i64, String)>,
    contexts: HashMap<String, String>,
    bytes: i64,
}

fn mapped(map: &HashMap<String, String>, value: &serde_json::Value) -> Result<String, ForkError> {
    value
        .as_str()
        .and_then(|v| map.get(v))
        .cloned()
        .ok_or_else(|| refuse("historical context references outside copied prefix").into())
}

impl Import<'_> {
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
                        v[4] = mapped(self.messages, &v[4])?.into();
                        v[5] = mapped(self.messages, &v[5])?.into();
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
                        let suffix = v[0]
                            .as_str()
                            .and_then(|key| {
                                key.strip_prefix(&format!("dcp.nudge.{}\0", self.source))
                            })
                            .ok_or_else(|| refuse("invalid historical nudge scope"))?;
                        v[0] = format!("dcp.nudge.{}\0{suffix}", self.root).into();
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
                    4 => 3,
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
        blocks: HashMap::new(),
        objects: HashMap::new(),
        contexts: HashMap::new(),
        bytes: bytes_left - bytes,
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
