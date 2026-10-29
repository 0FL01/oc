//! Deterministic SQLite work counters for storage scaling tests.
//!
//! A statement's VM steps depend only on the data and the plan, so comparing
//! the work of one operation at two history sizes catches scans without
//! timing. Counters are per thread: storage calls run on the test thread.
use rusqlite::{
    StatementStatus::VmStep,
    trace::{TraceEvent, TraceEventCodes},
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static STARTS: RefCell<HashMap<String, i32>> = RefCell::new(HashMap::new());
    static STEPS: Cell<i64> = const { Cell::new(0) };
    static BY_STATEMENT: RefCell<HashMap<String, i64>> = RefCell::new(HashMap::new());
}

/// `OC_VM_STEPS_DEBUG=1` prints the costliest statements of each measured operation.
fn debug() -> bool {
    std::env::var_os("OC_VM_STEPS_DEBUG").is_some()
}

fn count(event: TraceEvent<'_>) {
    match event {
        // Trigger subprograms report the same statement again; keep its start.
        TraceEvent::Stmt(stmt, _) => STARTS.with_borrow_mut(|starts| {
            starts
                .entry(stmt.sql().into_owned())
                .or_insert_with(|| stmt.get_status(VmStep));
        }),
        TraceEvent::Profile(stmt, _) => {
            if let Some(start) = STARTS.with_borrow_mut(|starts| starts.remove(stmt.sql().as_ref()))
            {
                let delta = i64::from(stmt.get_status(VmStep) - start);
                STEPS.set(STEPS.get() + delta);
                if debug() {
                    BY_STATEMENT.with_borrow_mut(|costs| {
                        *costs.entry(stmt.sql().into_owned()).or_default() += delta;
                    });
                }
            }
        }
        _ => {}
    }
}

/// Count every statement this connection runs from now on.
pub(crate) fn install(conn: &rusqlite::Connection) {
    conn.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT | TraceEventCodes::SQLITE_TRACE_PROFILE,
        Some(count),
    );
}

/// Result of `operation` and the VM steps its statements executed.
pub(crate) fn steps<T>(operation: impl FnOnce() -> T) -> (T, i64) {
    STARTS.with_borrow_mut(HashMap::clear);
    BY_STATEMENT.with_borrow_mut(HashMap::clear);
    STEPS.set(0);
    let result = operation();
    if debug() {
        BY_STATEMENT.with_borrow(|costs| {
            let mut costs: Vec<_> = costs.iter().collect();
            costs.sort_by_key(|(_, steps)| -**steps);
            for (sql, steps) in costs.into_iter().take(3) {
                let sql: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
                eprintln!("  {steps:>9} {}", &sql[..sql.len().min(400)]);
            }
        });
    }
    (result, STEPS.get())
}

/// Four times the history may add bounded index depth, not proportional work.
pub(crate) fn assert_flat(sizes: (usize, usize), before: &[(&str, i64)], after: &[(&str, i64)]) {
    for ((name, small), (_, large)) in before.iter().zip(after) {
        eprintln!(
            "{name}: {} -> {small} steps, {} -> {large} steps",
            sizes.0, sizes.1
        );
    }
    for ((name, small), (_, large)) in before.iter().zip(after) {
        assert!(
            *large <= small + small / 2 + 500,
            "{name}: {} took {small} VM steps, {} took {large}",
            sizes.0,
            sizes.1
        );
    }
}
