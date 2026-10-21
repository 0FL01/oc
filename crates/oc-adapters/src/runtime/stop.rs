//! Child-local stop provenance; the existing cancellation flag still owns IO.
//! Never hold this lock across Jobs/Db locks, awaits or terminal persistence.
use super::*;
use crate::tools::diagnostic::Diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    ChildInterrupt,
    SessionCancel,
    ParentUnknown,
    ApprovalRejected,
    QuestionUnknown,
    OriginCancelled,
    OwnerShutdown,
    StartStorageFailed,
    Unknown,
}

impl Cause {
    pub(super) fn diagnostic(self) -> Diagnostic {
        let (by, reason) = match self {
            Self::ChildInterrupt => ("user", "child_interrupt_requested"),
            Self::SessionCancel => ("user", "session_cancel_requested"),
            Self::ParentUnknown => ("parent", "parent_cancelled_unknown"),
            Self::ApprovalRejected => ("policy", "approval_rejected"),
            Self::QuestionUnknown => ("unknown", "question_cancelled_unknown"),
            Self::OriginCancelled => ("runtime", "native_origin_cancelled"),
            Self::OwnerShutdown => ("runtime", "owner_shutdown"),
            Self::StartStorageFailed => ("runtime", "child_start_storage_failed"),
            Self::Unknown => ("unknown", "cancellation_requested"),
        };
        Diagnostic::new(
            "subagent",
            if self == Self::StartStorageFailed {
                "infra_error"
            } else {
                "cancelled"
            },
            reason,
        )
        .detail(serde_json::json!({"by":by,"reason":reason}))
    }
}

#[derive(Default)]
struct State {
    cause: Option<Cause>,
    terminal: Option<Option<Diagnostic>>,
}

#[derive(Default)]
pub(super) struct Stop(Mutex<State>);

impl Stop {
    /// Source facts can precede the signal (e.g. plain approval rejection).
    pub(super) fn remember(&self, flag: &AtomicBool, cause: Cause) {
        let mut state = self.0.lock().expect("child stop");
        if state.terminal.is_none() && state.cause.is_none() {
            state.cause = Some(if flag.load(Ordering::Acquire) {
                Cause::Unknown
            } else {
                cause
            });
        }
    }

    pub(super) fn request(&self, flag: &AtomicBool, cause: Cause) {
        let mut state = self.0.lock().expect("child stop");
        if state.terminal.is_none() && state.cause.is_none() {
            // An already-observed bare signal cannot be attributed to a later,
            // unrelated explicit interrupt. Preserve honest unknown provenance.
            state.cause = Some(if flag.load(Ordering::Acquire) {
                Cause::Unknown
            } else {
                cause
            });
        }
        flag.store(true, Ordering::Release);
    }

    pub(super) fn freeze(&self, cancelled: bool) -> Option<Diagnostic> {
        let mut state = self.0.lock().expect("child stop");
        if state.terminal.is_none() {
            state.terminal =
                Some(cancelled.then(|| state.cause.unwrap_or(Cause::Unknown).diagnostic()));
        }
        state.terminal.as_ref().expect("terminal").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_precedes_signal_and_late_requests_never_rewrite_terminal_or_unknown() {
        let flag = AtomicBool::new(false);
        let stop = Stop::default();
        stop.remember(&flag, Cause::ApprovalRejected);
        stop.request(&flag, Cause::ParentUnknown);
        assert!(flag.load(Ordering::Acquire));
        let selected = stop.freeze(true).unwrap();
        assert_eq!(selected.detail["reason"], "approval_rejected");
        stop.request(&flag, Cause::OwnerShutdown);
        assert_eq!(stop.freeze(true), Some(selected));
        let completed = Stop::default();
        assert!(completed.freeze(false).is_none());
        completed.request(&AtomicBool::new(false), Cause::ChildInterrupt);
        assert!(completed.0.lock().unwrap().cause.is_none());
        assert!(completed.freeze(true).is_none());
        let bare = Stop::default();
        bare.request(&AtomicBool::new(true), Cause::ChildInterrupt);
        assert_eq!(bare.freeze(true).unwrap().detail["by"], "unknown");
        let late = Stop::default();
        late.remember(&AtomicBool::new(true), Cause::ApprovalRejected);
        assert_eq!(late.freeze(true).unwrap().detail["by"], "unknown");
    }
}
