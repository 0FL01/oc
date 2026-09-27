use crate::{dcp, provider::InputItem, storage::Db, tools::TurnLog};
use oc_core::{
    dcp_view::DcpRunSnapshot,
    queries::ModelRef,
    session::{Message, MessageId, Role},
};

pub(in crate::storage) fn complete(db: &Db, session: &str, turn: &str, with_call: bool) -> String {
    let text = "retained source fact ".repeat(512);
    let user = db
        .accept_turn(
            turn,
            session,
            &text,
            &text,
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap()
        .user_message;
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(user.clone());
    log.input
        .push(InputItem::message(crate::provider::InputRole::User, &text));
    if with_call {
        let op = format!("{turn}:read");
        log.display_parts.push(serde_json::json!({"tool": op}));
        log.input.push(InputItem::ProviderOutput(serde_json::json!({
            "type":"function_call", "call_id":"reused", "name":"read", "arguments":"{}"
        })));
        db.record_turn_tool_intent(&op, session, turn, "read", "{}", &log.to_json().to_string())
            .unwrap();
        db.record_tool_outcome(&op, "completed", Some("read result"))
            .unwrap();
        log.input.push(InputItem::FunctionCallOutput {
            call_id: "reused".into(),
            output: "read result".into(),
        });
    }
    let message_index = log.input.len();
    log.input.push(InputItem::message(
        crate::provider::InputRole::Assistant,
        "answer",
    ));
    log.display_parts
        .push(serde_json::json!({"message":message_index}));
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some("answer"),
    )
    .unwrap();
    user
}

pub(in crate::storage) fn standalone(
    db: &Db,
    session: &str,
    start: &str,
    end: &str,
    summary: &str,
    operation: Option<&str>,
) -> DcpRunSnapshot {
    let history = db
        .conversation_history_full(session)
        .unwrap()
        .into_iter()
        .map(|(id, role, text)| Message {
            id: MessageId(id),
            role: if role == "user" {
                Role::User
            } else {
                Role::Assistant
            },
            text,
        })
        .collect::<Vec<_>>();
    let preferences = [(
        format!("dcp.nudge.{session}\0fixture\0m"),
        "{\"turns_since_compress\":0}".into(),
    )];
    if let Some(op) = operation {
        db.record_tool_intent(op, session, None, "compress", "{}")
            .unwrap();
    }
    let metadata = operation.map(|operation_id| dcp::CompressionCommitMetadata {
        operation_id,
        operation_state: "completed",
        operation_output: "compressed",
        turn_id: None,
        turn_log: None,
        preference_updates: &preferences,
    });
    dcp::compress_ranges_atomic(
        db,
        session,
        &history,
        &[dcp::ValidatedRange {
            topic: "standalone durable cut".into(),
            start_id: start.into(),
            end_id: end.into(),
            summary: summary.into(),
        }],
        &oc_core::context_plan::ProtectedSpec::default(),
        metadata.as_ref(),
    )
    .unwrap()
    .snapshot
}
