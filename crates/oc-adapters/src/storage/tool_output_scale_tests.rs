//! Accumulated tool outputs, question results and finished shell/Code Mode
//! jobs must not make starting an output, opening one, the job-liveness probes
//! or the newest tool-operation page more expensive.
use super::vm_steps::{assert_flat, install, steps};
use super::*;
use oc_core::tool_output::CaptureState;

struct Population {
    _dir: tempfile::TempDir,
    db: Db,
    newest_path: String,
}

/// `ops` outputs with a question result each, plus as many finished jobs.
fn population(ops: usize) -> Population {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    // Population speed only: the measured statements do not depend on fsync.
    db.conn
        .skip_sync
        .store(true, std::sync::atomic::Ordering::SeqCst);
    db.create_session("s").unwrap();
    let mut newest_path = String::new();
    for i in 0..ops {
        for (op, tool) in [(format!("bash-{i}"), "bash"), (format!("ask-{i}"), "question")] {
            db.record_tool_intent(&op, "s", None, tool, "{}").unwrap();
            let mut writer = db
                .begin_tool_output(&op, "s", "/project", 1, "default", vec![])
                .unwrap();
            writer.append("output\n").unwrap();
            newest_path = writer.finish(false).unwrap().path;
            db.record_tool_outcome(&op, "completed", Some("{}")).unwrap();
        }
        db.record_tool_output_question(&format!("ask-{i}"), "s")
            .unwrap();
        let conn = db.conn.lock().unwrap();
        for table in ["shell_jobs", "execute_jobs"] {
            let background = if table == "execute_jobs" { ",background" } else { "" };
            let flag = if table == "execute_jobs" { ",0" } else { "" };
            conn.execute(
                &format!("INSERT INTO {table}(operation_id,session_id,version,provenance,phase,delivery_id{background}) VALUES(?1,'s',1,'{{}}','terminal',?2{flag})"),
                params![format!("bash-{i}"), format!("{table}-{i}")],
            )
            .unwrap();
        }
    }
    db.record_tool_intent("probe", "s", None, "bash", "{}")
        .unwrap();
    install(&db.conn.lock().unwrap());
    Population {
        _dir: dir,
        db,
        newest_path,
    }
}

fn measure(population: &Population) -> Vec<(&'static str, i64)> {
    let db = &population.db;
    let (writer, begin_steps) = steps(|| {
        db.begin_tool_output("probe", "s", "/project", 1, "default", vec![])
            .unwrap()
    });
    writer.abandon(CaptureState::Interrupted);
    let (reader, open_steps) = steps(|| db.open_tool_output("s", &population.newest_path));
    assert!(reader.is_ok());
    drop(reader);
    let (shell, shell_steps) = steps(|| db.has_active_shell_jobs("s").unwrap());
    assert!(!shell);
    let (execute, execute_steps) = steps(|| db.has_active_execute_jobs("s").unwrap());
    assert!(!execute);
    let (calls, calls_steps) = steps(|| db.live_execute_call_ids("s").unwrap());
    assert!(calls.is_empty());
    let (page, page_steps) = steps(|| db.list_tool_ops_page("s", 50, None).unwrap());
    assert_eq!(page.len(), 50);
    vec![
        ("begin_tool_output", begin_steps),
        ("open_tool_output", open_steps),
        ("has_active_shell_jobs", shell_steps),
        ("has_active_execute_jobs", execute_steps),
        ("live_execute_call_ids", calls_steps),
        ("list_tool_ops_page", page_steps),
    ]
}

#[test]
fn tool_output_and_job_probes_do_not_grow_with_accumulated_outputs() {
    let (small, large) = (100, 400);
    let before = measure(&population(small));
    let after = measure(&population(large));
    assert_flat((small, large), &before, &after);
}
