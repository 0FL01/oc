//! A database upgraded from the oldest supported schema has exactly the
//! schema of a fresh one: tables and columns, indexes, triggers and views.
use super::*;

/// One normalized line per column, index, trigger and view.
fn schema(db: &Db) -> Vec<String> {
    let conn = db.conn.lock().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut lines = Vec::new();
    for table in tables {
        let mut columns: Vec<String> = conn
            .prepare("SELECT name,type,\"notnull\",COALESCE(dflt_value,''),pk FROM pragma_table_info(?1)")
            .unwrap()
            .query_map([&table], |row| {
                Ok(format!(
                    "table {table}: {} {} notnull={} default={} pk={}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        columns.sort();
        lines.extend(columns);
    }
    let objects = conn
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE type IN ('index','trigger','view') AND sql IS NOT NULL ORDER BY type,name")
        .unwrap()
        .query_map([], |row| {
            let sql: String = row.get(3)?;
            Ok(format!(
                "{} {} on {}: {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                sql.split_whitespace().collect::<Vec<_>>().join(" ")
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    lines.extend(objects);
    lines
}

#[test]
fn a_database_upgraded_from_v1_has_the_fresh_schema() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_root = dir.path().join("legacy");
    fs::create_dir_all(&legacy_root).unwrap();
    // The oldest supported database: sessions without later columns,
    // migration 1 only (as `child_schema_upgrades_legacy_database_to_same_schema`).
    rusqlite::Connection::open(legacy_root.join("oc.sqlite"))
        .unwrap()
        .execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             CREATE TABLE sessions(id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
             INSERT INTO schema_migrations(version, applied_at) VALUES (1, 't04');
             INSERT INTO sessions(id, created_at) VALUES ('legacy', '1');",
        )
        .unwrap();
    let upgraded = schema(&Db::open(&legacy_root).unwrap());
    let fresh = schema(&Db::open(&dir.path().join("fresh")).unwrap());
    assert!(fresh.len() > 100, "{} schema lines", fresh.len());
    let missing: Vec<_> = fresh
        .iter()
        .filter(|line| !upgraded.contains(line))
        .collect();
    let extra: Vec<_> = upgraded
        .iter()
        .filter(|line| !fresh.contains(line))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the upgrade lacks {missing:#?}\nand has extra {extra:#?}"
    );
}

/// Opt-in: `OC_SCHEMA_COPY=<root>` names a copied data root (it is upgraded
/// in place) whose schema must then equal a fresh one.
#[test]
#[ignore = "needs a copied database"]
fn a_copied_database_upgrades_to_the_fresh_schema() {
    let root =
        std::path::PathBuf::from(std::env::var_os("OC_SCHEMA_COPY").expect("OC_SCHEMA_COPY"));
    let dir = tempfile::tempdir().unwrap();
    let upgraded = schema(&Db::open(&root).unwrap());
    let fresh = schema(&Db::open(&dir.path().join("fresh")).unwrap());
    let missing: Vec<_> = fresh
        .iter()
        .filter(|line| !upgraded.contains(line))
        .collect();
    let extra: Vec<_> = upgraded
        .iter()
        .filter(|line| !fresh.contains(line))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the upgrade lacks {missing:#?}\nand has extra {extra:#?}"
    );
}

fn fingerprint(lines: &[String]) -> u64 {
    lines
        .join("\n")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

/// Any change to a step of `Db::migrate` changes the fresh schema; existing
/// databases only run the chain again under a new `SCHEMA_CHAIN_VERSION`.
#[test]
fn schema_version_tracks_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = schema(&Db::open(dir.path()).unwrap());
    assert_eq!(
        (
            SCHEMA_CHAIN_VERSION,
            format!("{:016x}", fingerprint(&fresh))
        ),
        (21, "a2b47008c7c04c4b".to_owned()),
        "the schema changed: bump SCHEMA_CHAIN_VERSION and record the new fingerprint"
    );
}

/// The chain is skipped at the current version and runs again otherwise.
#[test]
fn the_chain_runs_only_below_the_current_version() {
    let dir = tempfile::tempdir().unwrap();
    let has_index = |db: &Db| -> bool {
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='events_tool_question')",
                [],
                |row| row.get(0),
            )
            .unwrap()
    };
    let db = Db::open(dir.path()).unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP INDEX events_tool_question")
        .unwrap();
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert!(!has_index(&db), "the current version skips the chain");
    db.conn
        .lock()
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_CHAIN_VERSION - 1)
        .unwrap();
    drop(db);
    assert!(has_index(&Db::open(dir.path()).unwrap()));
}

#[test]
fn w6_newer_schema_refuses_before_migrations_or_application_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("future").unwrap();
    drop(db);
    let conn = Connection::open(dir.path().join("oc.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE w6_future_marker(id INTEGER PRIMARY KEY, value BLOB NOT NULL);
         INSERT INTO w6_future_marker VALUES(1, X'5736006D61726B6572');
         DROP INDEX events_tool_question;
         INSERT INTO session_compactions VALUES('future-queued','future','{\"state\":\"queued\",\"marker\":\"unchanged\"}');",
    ).unwrap();
    let version = SCHEMA_CHAIN_VERSION + 1;
    conn.pragma_update(None, "user_version", version).unwrap();
    let schema = || {
        conn.prepare("SELECT type,name,sql FROM sqlite_master ORDER BY type,name")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let before = schema();
    let error = Db::open(dir.path())
        .err()
        .expect("a newer schema must refuse");
    let diagnostic = crate::application::storage_diagnostic(dir.path(), &error).to_string();
    assert!(diagnostic.contains(&format!("schema version {version}")));
    assert!(diagnostic.contains(&format!("supported version {SCHEMA_CHAIN_VERSION}")));
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        version
    );
    assert_eq!(schema(), before);
    assert_eq!(
        conn.query_row("SELECT value FROM w6_future_marker WHERE id=1", [], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .unwrap(),
        b"W6\0marker"
    );
    assert_eq!(
        conn.query_row(
            "SELECT snapshot FROM session_compactions WHERE id='future-queued'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        r#"{"state":"queued","marker":"unchanged"}"#
    );
}
