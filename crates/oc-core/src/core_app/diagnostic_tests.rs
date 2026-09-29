use super::*;
use crate::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage};

#[tokio::test]
async fn cfg10_worker_join_keeps_native_cause_and_redacts_legacy_failure() {
    let cause = ServiceDiagnostic {
        kind: ServiceKind::Runtime,
        service: "native".into(),
        source: "source-fixture/config".into(),
        field: vec!["mcp".into()],
        stage: ServiceStage::Cleanup,
        code: ServiceCode::CleanupFailed,
        action: ServiceAction::RestartApplication,
    };
    let returned = cause.clone();
    let worker = WorkerGuard::from_diagnostic_task(tokio::spawn(async move { Err(returned) }));
    assert_eq!(worker.join_diagnostic().await, Err(cause));
    let worker = WorkerGuard::from_task(tokio::spawn(async {
        Err("CFG10_LEGACY_AUTH_CANARY https://user:secret@private.invalid/key\u{1b}[31m".into())
    }));
    let error = worker.join().await.unwrap_err();
    assert!(error.contains("cleanup: runtime_failed") && error.contains("restart application"));
    assert!(!error.contains("CANARY") && !error.contains("https://") && !error.contains('\u{1b}'));
}

#[tokio::test]
async fn cfg10_worker_abort_is_cancelled_not_success() {
    let task = tokio::spawn(std::future::pending::<Result<(), ServiceDiagnostic>>());
    task.abort();
    let cause = WorkerGuard::from_diagnostic_task(task)
        .join_diagnostic()
        .await
        .unwrap_err();
    assert_eq!(cause.code, ServiceCode::Cancelled);
    assert_eq!(cause.stage, ServiceStage::Cleanup);
}
