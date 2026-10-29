use super::*;

fn request() -> QuestionRequest {
    QuestionRequest { id: 0, binding: ApprovalBinding { session: "s".into(), turn: "t".into(), call: "c".into(), operation: "o".into(), input_digest: "digest".into(), location: "l".into(), generation: 7, agent: None, agent_digest: None }, input: QuestionInput::parse(&serde_json::json!({"questions":[{"question":"Choose?","header":"Choice","options":[{"label":"One","description":"first"}]}]})).unwrap(), images: false }
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
        decision: QuestionDecision::Answers {
            answers: vec![vec!["One".into()]],
            images: vec![],
        },
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
                decision: QuestionDecision::Answers {
                    answers: vec![],
                    images: vec![]
                },
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

#[tokio::test]
async fn explicit_reload_distinctly_settles_only_the_open_waiter() {
    let queue = Arc::new(QuestionQueue::default());
    queue.register_consumer();
    let (events, mut rx) = broadcast::channel(8);
    let job = tokio::spawn({
        let queue = queue.clone();
        let events = events.clone();
        async move { queue.wait(request(), &events).await }
    });
    let CoreEvent::QuestionAsked(request) = rx.recv().await.unwrap() else {
        panic!("question readiness");
    };
    queue.reload(Some(&request.binding.session), &events);
    assert_eq!(job.await.unwrap(), Ok(QuestionDecision::Reloaded));
    assert!(matches!(
        rx.recv().await.unwrap(),
        CoreEvent::QuestionResolved {
            decision: QuestionDecision::Reloaded,
            ..
        }
    ));
    assert!(queue.pending().is_empty());
}
#[test]
fn answer_images_require_capability_label_and_budget() {
    let image = |question, answer, start| QuestionImage {
        question,
        answer,
        mark: crate::prompt::ImageMark {
            start,
            end: start + 9,
            ordinal: 1,
        },
        png: b"opaque".as_slice().into(),
    };
    let answers = vec![vec!["see [Image 1]".to_owned()]];
    let mut request = request();
    assert_eq!(
        request.validate_decision(&answers, &[image(0, 0, 4)]),
        Err(QuestionReplyError::InvalidAnswers),
        "a model without image input never receives pasted images"
    );
    request.images = true;
    assert!(
        request
            .validate_decision(&answers, &[image(0, 0, 4)])
            .is_ok()
    );
    for wrong in [image(0, 0, 3), image(1, 0, 4), image(0, 1, 4)] {
        assert!(request.validate_decision(&answers, &[wrong]).is_err());
    }
    assert!(
        request
            .validate_decision(&answers, &[image(0, 0, 4), image(0, 0, 4)])
            .is_err(),
        "duplicate ordinal"
    );
    let mut large = image(0, 0, 4);
    large.png = vec![0; crate::prompt::IMAGE_BYTES + 1].into();
    assert!(request.validate_decision(&answers, &[large]).is_err());
    let decision = QuestionDecision::Answers {
        answers: answers.clone(),
        images: vec![image(0, 0, 4)],
    };
    let wire = serde_json::to_string(&decision).unwrap();
    assert!(!wire.contains("opaque"), "PNG bytes travel base64-encoded");
    assert_eq!(
        serde_json::from_str::<QuestionDecision>(&wire).unwrap(),
        decision
    );
}
#[tokio::test]
async fn resolved_event_omits_pasted_image_bytes() {
    let queue = Arc::new(QuestionQueue::default());
    let (events, mut rx) = broadcast::channel(8);
    queue.register_consumer();
    let mut asked = request();
    asked.images = true;
    let (q, e) = (queue.clone(), events.clone());
    let job = tokio::spawn(async move { q.wait(asked, &e).await });
    let CoreEvent::QuestionAsked(r) = rx.recv().await.unwrap() else {
        panic!("asked");
    };
    let decision = QuestionDecision::Answers {
        answers: vec![vec!["[Image 1]".into()]],
        images: vec![QuestionImage {
            question: 0,
            answer: 0,
            mark: crate::prompt::ImageMark {
                start: 0,
                end: 9,
                ordinal: 1,
            },
            png: b"opaque".as_slice().into(),
        }],
    };
    queue
        .resolve(
            QuestionReply {
                id: r.id,
                binding: r.binding,
                decision: decision.clone(),
            },
            &events,
        )
        .unwrap();
    let CoreEvent::QuestionResolved {
        decision: published,
        ..
    } = rx.recv().await.unwrap()
    else {
        panic!("resolved");
    };
    assert_eq!(
        published,
        QuestionDecision::Answers {
            answers: vec![vec!["[Image 1]".into()]],
            images: vec![]
        }
    );
    assert_eq!(job.await.unwrap(), Ok(decision), "the tool still gets them");
}
