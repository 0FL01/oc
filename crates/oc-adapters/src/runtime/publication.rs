//! Source/MCP commit ownership and coherent fresh request capture.
//!
//! The supervisor owns an accepted commit even if its caller loses the ACK. The
//! short synchronous gate covers only desired-state reconciliation and Arc swaps,
//! never candidate construction, discovery, provider IO or issued tool execution.
use super::*;

pub(super) struct Commit {
    pub(super) gate: Arc<Mutex<()>>,
    current: Arc<RwLock<Arc<ExecutionGeneration>>>,
    expected: Arc<ExecutionGeneration>,
    candidate: Arc<ExecutionGeneration>,
}

impl Commit {
    pub(super) fn admitted(&self) -> bool {
        Arc::ptr_eq(
            &self.current.read().expect("generation lock"),
            &self.expected,
        )
    }

    pub(super) fn publish(&self) {
        *self.current.write().expect("generation lock") = self.candidate.clone();
    }
}

impl Runtime<'_> {
    pub(super) fn source_commit(
        &self,
        expected: Arc<ExecutionGeneration>,
        candidate: ExecutionGeneration,
    ) -> Commit {
        Commit {
            gate: self.publication_commit.clone(),
            current: self.current.clone(),
            expected,
            candidate: Arc::new(candidate),
        }
    }

    /// Capture services and source together before any fresh preparation awaits.
    pub(super) fn fresh_request_view(
        &self,
    ) -> Result<(Arc<ExecutionGeneration>, Arc<McpGeneration>), RuntimeError> {
        let _commit = self.publication_commit.lock().expect("publication commit");
        let published = self.current.read().expect("generation lock").clone();
        let attached = self.mcp_view_in(&published)?;
        Ok((published, attached))
    }
}
