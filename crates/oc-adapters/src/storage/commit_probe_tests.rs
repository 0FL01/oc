//! Opt-in durable write throughput probe on the real disk:
//! `cargo test -p oc-adapters --lib commit_probe -- --ignored --nocapture`.
use super::*;
use std::sync::atomic::Ordering;
use std::time::Instant;

#[test]
#[ignore = "opt-in disk throughput probe"]
fn commit_probe() {
    for threads in [1, 8] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_session("s").unwrap();
        let syncs = db.conn.syncs.load(Ordering::SeqCst);
        let start = Instant::now();
        std::thread::scope(|scope| {
            for t in 0..threads {
                let db = db.shared_handle();
                scope.spawn(move || {
                    for i in 0..50 {
                        let op = format!("op-{t}-{i}");
                        db.record_tool_intent(&op, "s", None, "read", "{}").unwrap();
                        db.record_tool_outcome(&op, "completed", Some("ok"))
                            .unwrap();
                    }
                });
            }
        });
        let writes = threads * 100;
        eprintln!(
            "threads={threads}: {:.0} durable writes/s, {} WAL fsyncs for {writes} writes",
            writes as f64 / start.elapsed().as_secs_f64(),
            db.conn.syncs.load(Ordering::SeqCst) - syncs
        );
    }
}
