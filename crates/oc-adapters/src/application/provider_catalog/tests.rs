use super::*;

#[tokio::test]
async fn ui07_completed_catalog_join_failure_is_not_suppressed_on_stop() {
    let task: JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>> =
        tokio::spawn(async { panic!("owned catalog fixture join failure") });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut work = ProviderWork { task: Some(task) };
    let result = work.stop().await;
    assert_eq!(result, Err("native catalog worker failed".into()));
}
