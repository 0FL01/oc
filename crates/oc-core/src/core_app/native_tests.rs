use super::*;

#[tokio::test]
async fn srv04_pre_ack_cancel_crosses_full_work_inbox_without_polling_it() {
    let (app, mut inbox, mut control, _, admissions) = CoreApp::native_channel(MAX_QUEUE_ITEMS, 1);
    let mut receipts = Vec::new();
    for index in 0..MAX_QUEUE_ITEMS {
        receipts.push(
            app.request_submit_fresh(SessionId(format!("queued-{index}")), "input".into(), None)
                .unwrap(),
        );
    }
    assert_eq!(inbox.len(), MAX_QUEUE_ITEMS);
    assert!(matches!(
        app.request_submit_fresh(SessionId("overflow".into()), "input".into(), None),
        Err(CoreError::QueueFull)
    ));
    let request = app.cancel(SessionId("queued-0".into()));
    let owner = async {
        let NativeControl::Cancel {
            session,
            admission: Some(admission),
            ack,
            ..
        } = control.recv().await.unwrap()
        else {
            panic!("missing exact pre-ACK cancellation");
        };
        assert_eq!(session.0, "queued-0");
        assert!(admission.cancelled.load(Ordering::Acquire));
        assert_eq!(
            admission.request.load(Ordering::Acquire),
            0,
            "not yet dispatched"
        );
        assert_eq!(
            inbox.len(),
            MAX_QUEUE_ITEMS,
            "control must not wait for a work slot"
        );
        ack.send(Ok(())).unwrap();
    };
    let (cancelled, ()) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(request, owner)
    })
    .await
    .unwrap();
    cancelled.unwrap();
    let message = inbox.recv().await.unwrap();
    let InboxMsg::SubmitFresh { session, .. } = message else {
        panic!("first ordered request changed");
    };
    let first = admissions.claim(&session).unwrap();
    assert!(first.cancelled.load(Ordering::Acquire));
    admissions.complete(&first);
    let next = admissions.claim(&SessionId("queued-1".into())).unwrap();
    assert!(
        !next.cancelled.load(Ordering::Acquire),
        "cancellation cannot bleed into the next input"
    );
}
