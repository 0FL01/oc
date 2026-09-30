use super::*;

fn request() -> QuestionRequest {
    QuestionRequest { id: 0, binding: ApprovalBinding { session: "s".into(), turn: "t".into(), call: "c".into(), operation: "o".into(), input_digest: "digest".into(), location: "l".into(), generation: 7, agent: None, agent_digest: None }, input: QuestionInput::parse(&serde_json::json!({"questions":[{"question":"Choose?","header":"Choice","options":[{"label":"One","description":"first"}]}]})).unwrap() }
}
#[test]
fn strict_schema_and_typed_history_are_bounded() {
    let mut value = serde_json::to_value(request().input).unwrap();
    assert!(!QuestionInput::parse(&value).unwrap().questions[0].multiple);
    value["questions"][0]["multiple"] = serde_json::json!("false");
    assert!(QuestionInput::parse(&value).is_err());
    value["questions"] = serde_json::json!([]);
    assert!(QuestionInput::parse(&value).is_err());
    let result = QuestionResult {
        questions: request().input.questions,
        answers: vec![vec!["custom".into()]],
    };
    let output = serde_json::to_string(&result).unwrap();
    assert_eq!(
        QuestionResult::from_output("question", "completed", Some(&output)),
        Some(result)
    );
    assert!(QuestionResult::from_output("shell", "completed", Some(&output)).is_none());
    assert!(QuestionResult::from_output("question", "unknown", Some(&output)).is_none());
    assert!(
        request()
            .input
            .validate_answers(&[vec!["x".repeat(ANSWER_BYTES_CAP + 1)]])
            .is_err()
    );
    assert!(
        request()
            .input
            .validate_answers(&[vec!["a".into(), "b".into()]])
            .is_err()
    );
}
#[tokio::test]
async fn wait_reply_binding_errors_preserve_then_settle_once() {
    let queue = Arc::new(QuestionQueue::default());
    let (events, mut rx) = broadcast::channel(32);
    assert!(queue.wait(request(), &events).await.is_err());
    queue.register_consumer();
    let q = queue.clone();
    let e = events.clone();
    let job = tokio::spawn(async move { q.wait(request(), &e).await });
    let CoreEvent::QuestionAsked(r) = rx.recv().await.unwrap() else {
        panic!("question readiness");
    };
    let good = QuestionReply {
        id: r.id,
        binding: r.binding.clone(),
        decision: QuestionDecision::Answers(vec![vec!["One".into()]]),
    };
    for binding in [
        ApprovalBinding {
            session: "foreign".into(),
            ..r.binding.clone()
        },
        ApprovalBinding {
            turn: "foreign".into(),
            ..r.binding.clone()
        },
        ApprovalBinding {
            call: "foreign".into(),
            ..r.binding.clone()
        },
        ApprovalBinding {
            operation: "foreign".into(),
            ..r.binding.clone()
        },
        ApprovalBinding {
            generation: 8,
            ..r.binding.clone()
        },
        ApprovalBinding {
            input_digest: "foreign".into(),
            ..r.binding.clone()
        },
    ] {
        assert_eq!(
            queue.resolve(
                QuestionReply {
                    binding,
                    ..good.clone()
                },
                &events
            ),
            Err(QuestionReplyError::BindingMismatch)
        );
    }
    assert_eq!(
        queue.resolve(
            QuestionReply {
                decision: QuestionDecision::Answers(vec![]),
                ..good.clone()
            },
            &events
        ),
        Err(QuestionReplyError::InvalidAnswers)
    );
    assert_eq!(queue.pending(), vec![r]);
    queue.resolve(good.clone(), &events).unwrap();
    assert_eq!(job.await.unwrap().unwrap(), good.decision);
    assert_eq!(queue.resolve(good, &events), Err(QuestionReplyError::Stale));
    assert!(queue.pending().is_empty());
}
#[tokio::test]
async fn dropped_waiter_retires_and_late_reply_is_refused() {
    let queue = Arc::new(QuestionQueue::default());
    queue.register_consumer();
    let (events, mut rx) = broadcast::channel(32);
    let q = queue.clone();
    let e = events.clone();
    let job = tokio::spawn(async move { q.wait(request(), &e).await });
    let CoreEvent::QuestionAsked(r) = rx.recv().await.unwrap() else {
        panic!("readiness");
    };
    job.abort();
    assert!(job.await.unwrap_err().is_cancelled());
    assert!(queue.pending().is_empty());
    assert_eq!(
        queue.resolve(
            QuestionReply {
                id: r.id,
                binding: r.binding,
                decision: QuestionDecision::Cancelled
            },
            &events
        ),
        Err(QuestionReplyError::Stale)
    );
}

#[tokio::test]
async fn pending_forms_have_a_hard_bound_and_all_waiters_retire() {
    let queue = Arc::new(QuestionQueue::default());
    queue.register_consumer();
    let (events, mut rx) = broadcast::channel(256);
    let mut jobs = Vec::new();
    for _ in 0..64 {
        let q = queue.clone();
        let e = events.clone();
        jobs.push(tokio::spawn(async move { q.wait(request(), &e).await }));
        assert!(matches!(
            rx.recv().await.unwrap(),
            CoreEvent::QuestionAsked(_)
        ));
    }
    assert_eq!(
        queue.wait(request(), &events).await,
        Err(QuestionWaitError::Capacity)
    );
    assert_eq!(queue.pending().len(), 64);
    queue.cancel(None, &events);
    for job in jobs {
        assert_eq!(job.await.unwrap(), Ok(QuestionDecision::Cancelled));
    }
    assert!(queue.pending().is_empty());
}
