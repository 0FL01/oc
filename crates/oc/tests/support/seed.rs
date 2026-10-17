//! Prelaunch fixture writes only: preserve owner rows/events in one durable
//! transaction, not thousands of separately acknowledged runtime mutations.
use rusqlite::{Connection, params};
use std::path::Path;

pub(super) fn connection(data_dir: &Path) -> Connection {
    let conn = Connection::open(data_dir.join("oc.sqlite")).expect("fixture database");
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
        .expect("fixture storage guarantees");
    conn
}

pub(super) fn messages(
    conn: &Connection,
    session: &str,
    rows: impl Iterator<Item = (&'static str, String)>,
) -> Vec<String> {
    let began = std::time::Instant::now();
    let tx = conn
        .unchecked_transaction()
        .expect("fixture message transaction");
    let mut seq: i64 = tx
        .query_row("SELECT COALESCE(MAX(seq), 0) FROM messages", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut ids = Vec::new();
    {
        let mut message = tx
            .prepare("INSERT INTO messages(id,session_id,seq,role,text) VALUES(?1,?2,?3,?4,?5)")
            .unwrap();
        let mut event = tx
            .prepare("INSERT INTO events(session_id,kind,payload) VALUES(?1,'message',?2)")
            .unwrap();
        for (role, text) in rows {
            seq += 1;
            let id = format!("m{seq:04}");
            message
                .execute(params![id, session, seq, role, text])
                .unwrap();
            event.execute(params![session, id]).unwrap();
            ids.push(id);
        }
    }
    tx.commit().expect("durable fixture messages");
    eprintln!(
        "fixture seed session={session} messages={} elapsed={:?}",
        ids.len(),
        began.elapsed()
    );
    ids
}
