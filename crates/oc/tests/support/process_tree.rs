//! Resource probes must count the native owner as well as the visible client.
//! Registration is read only inside the fixture's isolated data directory.
use std::{collections::BTreeSet, path::Path};

pub struct ProcessTree {
    pub pids: BTreeSet<u32>,
    pub owner: Option<u32>,
}

pub fn capture(client: u32, data: &Path) -> ProcessTree {
    let owner = std::fs::read(data.join("native-owner.json"))
        .ok()
        .filter(|bytes| bytes.len() <= 4096)
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["pid"].as_u64())
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|&pid| pid != client && Path::new(&format!("/proc/{pid}")).exists());
    let mut pending = vec![client];
    pending.extend(owner);
    let mut pids = BTreeSet::new();
    while let Some(pid) = pending.pop() {
        if !pids.insert(pid) {
            continue;
        }
        // Tokio may launch a child from any worker thread, not just the leader.
        if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                if let Ok(children) = std::fs::read_to_string(task.path().join("children")) {
                    pending.extend(
                        children
                            .split_whitespace()
                            .filter_map(|pid| pid.parse::<u32>().ok()),
                    );
                }
            }
        }
    }
    ProcessTree { pids, owner }
}
