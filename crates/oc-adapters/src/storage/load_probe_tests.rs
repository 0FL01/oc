//! Opt-in storage load probes (`cargo test -p oc-adapters --lib load_probe --
//! --ignored --nocapture --test-threads=1`). They print timings on the real
//! disk with real fsyncs; they assert only result shapes, not timings.
use super::children::scale_tests::admit;
use super::*;
use oc_core::queries::ModelRef;
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn percentile(samples: &mut [Duration], p: f64) -> Duration {
    samples.sort();
    samples[((samples.len() - 1) as f64 * p).round() as usize]
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn quiet(db: &Db, skip: bool) {
    db.conn
        .skip_sync
        .store(skip, std::sync::atomic::Ordering::SeqCst);
}

/// 1k and 10k accumulated child jobs (half of them background, delivered):
/// launch latency with real fsyncs and the newest history page of the parent.
#[test]
#[ignore = "opt-in load probe"]
fn load_probe_child_jobs() {
    for jobs in [1_000, 10_000] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_session("parent").unwrap();
        quiet(&db, true);
        let started = Instant::now();
        for i in 0..jobs {
            let job = admit(&db, i, i % 2 == 1);
            db.finish_child_job(
                &job.operation,
                oc_core::queries::ChildState::Completed,
                "done",
            )
            .unwrap();
            db.deliver_child_notices().unwrap();
        }
        let populated = started.elapsed();
        quiet(&db, false);
        let before = db.storage_metrics();
        let mut launches = Vec::new();
        for i in jobs..jobs + 100 {
            let since = Instant::now();
            let job = launch(&db, i);
            launches.push(since.elapsed());
            db.finish_child_job(
                &job.operation,
                oc_core::queries::ChildState::Completed,
                "done",
            )
            .unwrap();
            db.deliver_child_notices().unwrap();
        }
        let after = db.storage_metrics();
        eprintln!(
            "launch writes: commits={} fsyncs={} fsync_us={} durable_wait_us={} checkpoints={}",
            after.commits - before.commits,
            after.fsyncs - before.fsyncs,
            after.fsync_us - before.fsync_us,
            after.durable_wait_us - before.durable_wait_us,
            after.checkpoints - before.checkpoints
        );
        let mut pages: Vec<Duration> = (0..20)
            .map(|_| {
                let since = Instant::now();
                let page = db.read_history_page_typed("parent", 100, None).unwrap();
                for row in &page {
                    db.child_history("parent", &row.id).unwrap();
                }
                assert!(!page.is_empty());
                db.child_tool_jobs("parent").unwrap();
                since.elapsed()
            })
            .collect();
        let since = Instant::now();
        let outstanding = db.child_job_outstanding().unwrap();
        let outstanding_time = since.elapsed();
        assert_eq!(outstanding, 0);
        eprintln!(
            "jobs={jobs}: populated in {:.1}s; launch p50={:.2}ms p95={:.2}ms; history page+classification p50={:.2}ms p95={:.2}ms; outstanding {:.2}ms",
            populated.as_secs_f64(),
            ms(percentile(&mut launches, 0.5)),
            ms(percentile(&mut launches, 0.95)),
            ms(percentile(&mut pages, 0.5)),
            ms(percentile(&mut pages, 0.95)),
            ms(outstanding_time),
        );
    }
}

/// The owner's durable launch commits: the tool intent, the fresh child
/// session with its admitted job (one commit), and the running outcome.
fn launch(db: &Db, i: usize) -> oc_core::queries::ChildJob {
    let mut job = super::children::scale_tests::job(i, true);
    job.location = "/project".into();
    let input = json!({"agent":"helper","prompt":"task","background":true});
    db.record_tool_intent(
        &job.operation,
        "parent",
        None,
        "subagent",
        &input.to_string(),
    )
    .unwrap();
    db.admit_recoverable_child_job(&job, true, &json!({"launch": i}))
        .unwrap();
    let output = json!({"status":"running","sessionID":job.child.0,"jobGeneration":job.operation,"deliveryID":job.delivery_id});
    db.record_tool_outcome(&job.operation, "running", Some(&output.to_string()))
        .unwrap();
    job
}

/// 1/4/8/16 sessions run turns concurrently as tasks of a two-worker runtime,
/// like the owner: each turn accepts, runs five tool intents/outcomes with a
/// checkpoint each, and settles. Reports acknowledged-write latency,
/// throughput, timer lag of an unrelated task and the writer counters.
#[test]
#[ignore = "opt-in load probe"]
fn load_probe_sessions() {
    const TURNS: usize = 10;
    const TOOLS: usize = 5;
    for sessions in [1, 4, 8, 16] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        for s in 0..sessions {
            db.create_session(&format!("s{s}")).unwrap();
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let before = db.storage_metrics();
        let started = Instant::now();
        let (mut acks, mut lags) = runtime.block_on(async {
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let ticker = {
                let done = done.clone();
                tokio::spawn(async move {
                    let mut lags = Vec::new();
                    while !done.load(std::sync::atomic::Ordering::SeqCst) {
                        let since = Instant::now();
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        lags.push(since.elapsed().saturating_sub(Duration::from_millis(5)));
                    }
                    lags
                })
            };
            let tasks: Vec<_> = (0..sessions)
                .map(|s| {
                    let db = db.shared_handle();
                    tokio::spawn(async move { session_turns(&db, &format!("s{s}"), TURNS, TOOLS) })
                })
                .collect();
            let mut acks = Vec::new();
            for task in tasks {
                acks.extend(task.await.unwrap());
            }
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            (acks, ticker.await.unwrap())
        });
        let elapsed = started.elapsed();
        let after = db.storage_metrics();
        let writes = acks.len();
        eprintln!(
            "sessions={sessions}: {writes} writes in {:.2}s = {:.0}/s; ack p50={:.2}ms p99={:.2}ms max={:.2}ms; timer lag p99={:.2}ms max={:.2}ms; fsyncs={} commits={} mean_hold={}us max_runtime_hold={}us checkpoints={} max_checkpoint={}us stalled_waits={}",
            elapsed.as_secs_f64(),
            writes as f64 / elapsed.as_secs_f64(),
            ms(percentile(&mut acks, 0.5)),
            ms(percentile(&mut acks, 0.99)),
            ms(percentile(&mut acks, 1.0)),
            ms(percentile(&mut lags, 0.99)),
            ms(percentile(&mut lags, 1.0)),
            after.fsyncs - before.fsyncs,
            after.commits - before.commits,
            (after.hold_us - before.hold_us) / (after.locks - before.locks).max(1),
            after.max_runtime_hold_us,
            after.checkpoints - before.checkpoints,
            after.max_checkpoint_us,
            after.stalled_waits,
        );
    }
}

/// One session's turns; the latency of every acknowledged write.
fn session_turns(db: &Db, session: &str, turns: usize, tools: usize) -> Vec<Duration> {
    let model = ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let mut acks = Vec::new();
    let mut timed = |write: &mut dyn FnMut()| {
        let since = Instant::now();
        write();
        acks.push(since.elapsed());
    };
    for t in 0..turns {
        let turn = format!("{session}-turn-{t}");
        timed(&mut || {
            db.accept_turn(&turn, session, "question", "question", &model)
                .unwrap();
        });
        let mut parts = Vec::new();
        for k in 0..tools {
            let op = format!("{turn}-op-{k}");
            let input = json!({"path": format!("src/f{k}.rs")}).to_string();
            timed(&mut || {
                db.record_tool_intent(&op, session, Some(&turn), "read", &input)
                    .unwrap();
            });
            timed(&mut || {
                db.record_tool_outcome(&op, "completed", Some(&"content ".repeat(512)))
                    .unwrap();
            });
            parts.push(json!({"tool": op}));
            let log = json!({"display_parts": parts}).to_string();
            timed(&mut || db.checkpoint_turn(&turn, &log).unwrap());
        }
        let log = json!({"display_parts": parts}).to_string();
        timed(&mut || {
            db.commit_turn(&turn, "completed", Some(&log), Some("answer"))
                .unwrap();
        });
    }
    acks
}

/// Opt-in: `OC_PROBE_COPY=<copied data root>` (upgraded in place). The newest
/// History window of the session with the largest journals, projected per row
/// as the application does.
#[test]
#[ignore = "needs a copied database"]
fn load_probe_history_on_copy() {
    let root = std::path::PathBuf::from(std::env::var_os("OC_PROBE_COPY").expect("OC_PROBE_COPY"));
    let db = Db::open(&root).unwrap();
    let (session, bytes): (String, i64) = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT session_id,sum(length(result)) AS bytes FROM turns GROUP BY session_id ORDER BY bytes DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let window = oc_core::queries::HistoryReadWindow {
        before_seq: None,
        after_seq: None,
        part: None,
    };
    let mut parts = [Duration::ZERO; 6];
    let mut timed = |part: usize, run: &mut dyn FnMut()| {
        let since = Instant::now();
        run();
        parts[part] += since.elapsed();
    };
    let mut pages: Vec<Duration> = (0..10)
        .map(|_| {
            let since = Instant::now();
            let mut rows = Vec::new();
            timed(0, &mut || {
                rows = db.read_transcript_rows(&session, &window, 100).unwrap()
            });
            for row in rows {
                timed(1, &mut || {
                    drop(db.user_shell_result(&session, &row.id).unwrap())
                });
                timed(2, &mut || {
                    drop(db.child_history(&session, &row.id).unwrap())
                });
                timed(3, &mut || {
                    drop(db.model_shell_notice(&session, &row.id).unwrap())
                });
                timed(4, &mut || {
                    drop(db.user_images(&session, &row.id.0, &row.text).unwrap())
                });
                timed(5, &mut || {
                    drop(
                        db.history_turn_window(&session, row.seq, None, false)
                            .unwrap(),
                    )
                });
            }
            since.elapsed()
        })
        .collect();
    if std::env::var_os("OC_PROBE_PROFILE").is_some() {
        db.conn.lock().unwrap().trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_PROFILE,
            Some(profile),
        );
        for row in db.read_transcript_rows(&session, &window, 100).unwrap() {
            db.model_shell_notice(&session, &row.id).unwrap();
            db.history_turn_window(&session, row.seq, None, false)
                .unwrap();
        }
        PROFILE.with_borrow(|costs| {
            let mut costs: Vec<_> = costs.iter().collect();
            costs.sort_by_key(|(_, (time, _))| std::cmp::Reverse(*time));
            for (sql, (time, calls)) in costs.into_iter().take(6) {
                let sql: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
                eprintln!(
                    "  {:>8.1}ms {calls:>4}x {}",
                    ms(*time),
                    &sql[..sql.len().min(300)]
                );
            }
        });
    }
    for (name, total) in [
        "transcript",
        "user_shell",
        "child",
        "shell_notice",
        "images",
        "turn_window",
    ]
    .iter()
    .zip(parts)
    {
        eprintln!("  {name}: {:.1}ms per window", ms(total) / 10.0);
    }
    eprintln!(
        "session with {} MiB of journals: history window p50={:.1}ms p95={:.1}ms",
        bytes >> 20,
        ms(percentile(&mut pages, 0.5)),
        ms(percentile(&mut pages, 0.95)),
    );
}

thread_local! {
    static PROFILE: std::cell::RefCell<HashMap<String, (Duration, usize)>> = Default::default();
}

fn profile(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Profile(stmt, time) = event {
        PROFILE.with_borrow_mut(|costs| {
            let cost = costs.entry(stmt.sql().into_owned()).or_default();
            cost.0 += time;
            cost.1 += 1;
        });
    }
}

/// Opt-in: WAL pages and time of a tracked DCP preference update (the most
/// frequent versioned mutation) with and without the versioning triggers.
#[test]
#[ignore = "opt-in load probe"]
fn load_probe_dcp_versioning() {
    let run = |tracked: bool| {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        quiet(&db, true);
        db.create_session("s").unwrap();
        if !tracked {
            let conn = db.conn.lock().unwrap();
            let triggers: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'conversation_track_%'")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for trigger in triggers {
                conn.execute_batch(&format!("DROP TRIGGER {trigger}"))
                    .unwrap();
            }
        }
        let before = db.storage_metrics().wal_frames;
        let since = Instant::now();
        for k in 0..1000 {
            db.set_pref(
                "dcp.nudge.s\0fixture\0m",
                &format!("{{\"turns_since_compress\":{k}}}"),
            )
            .unwrap();
        }
        (db.storage_metrics().wal_frames - before, since.elapsed())
    };
    let (tracked, untracked) = (run(true), run(false));
    eprintln!(
        "1000 tracked pref updates: {} WAL pages {:.0}ms; untracked: {} pages {:.0}ms",
        tracked.0,
        ms(tracked.1),
        untracked.0,
        ms(untracked.1)
    );
}

/// Opt-in: time of opening a fresh and an existing data root.
#[test]
#[ignore = "opt-in load probe"]
fn load_probe_open() {
    let dir = tempfile::tempdir().unwrap();
    let since = Instant::now();
    drop(Db::open(&dir.path().join("a")).unwrap());
    let fresh = since.elapsed();
    let since = Instant::now();
    drop(Db::open(&dir.path().join("a")).unwrap());
    eprintln!(
        "open fresh {:.0}ms, existing {:.0}ms",
        ms(fresh),
        ms(since.elapsed())
    );
}
