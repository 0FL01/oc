//! Opt-in storage load probes (`cargo test -p oc-adapters --lib load_probe --
//! --ignored --nocapture --test-threads=1`). They print timings on the real
//! disk with real fsyncs; they assert only result shapes, not timings.
use super::children::scale_tests::admit;
use super::*;
use oc_core::queries::ModelRef;
use serde_json::json;
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
