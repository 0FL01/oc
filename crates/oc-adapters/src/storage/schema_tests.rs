//! A database upgraded from the oldest supported schema has exactly the
//! schema of a fresh one: tables and columns, indexes, triggers and views.
use super::*;

/// One normalized line per column, index, trigger and view, after the
/// schema the runtime adds before its first turn.
fn schema(db: &Db) -> Vec<String> {
    crate::dcp::apply_dcp_schema(db).unwrap();
    db.grants_schema().unwrap();
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
