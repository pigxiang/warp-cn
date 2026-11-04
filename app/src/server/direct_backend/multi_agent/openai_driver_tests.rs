//! Tests for the OpenAI wire projection: the shapes that strict gateways
//! (GLM on Volcengine Ark) reject with an opaque `InvalidParameter` 400.

use serde_json::{Value, json};

use super::{message_shape, project_messages};
use crate::server::direct_backend::multi_agent::decode::{NormalizedTurn, ToolResult, ToolUse};
use crate::server::direct_backend::multi_agent::tool_schema::ToolKind;

fn assistant_message(messages: &[Value]) -> &Value {
    messages
        .iter()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
        .expect("assistant message present")
}

/// The turn shape a reasoning model produces before calling tools: CoT in
/// `reasoning`, no visible text, tool calls attached.
fn tool_call_only_assistant() -> NormalizedTurn {
    NormalizedTurn::Assistant {
        text: None,
        tool_uses: vec![ToolUse {
            tool_use_id: "call_1".into(),
            tool_kind: ToolKind::RunShellCommand,
            input: json!({"command": "ls -la"}),
        }],
        reasoning: Some("Let me explore the workspace first.".into()),
        message_ids: vec![],
    }
}

#[test]
fn tool_call_turn_omits_content_instead_of_sending_null() {
    let messages = project_messages(&[tool_call_only_assistant()], "system");

    let assistant = assistant_message(&messages);
    assert!(
        assistant.get("content").is_none(),
        "an explicit null content makes Ark reject the request: {assistant}"
    );
    assert_eq!(
        assistant
            .get("tool_calls")
            .and_then(|c| c.as_array())
            .map(Vec::len),
        Some(1)
    );
    // The CoT still round-trips; reasoning models need it echoed back.
    assert_eq!(
        assistant.get("reasoning_content").and_then(|c| c.as_str()),
        Some("Let me explore the workspace first.")
    );
}

#[test]
fn tool_call_turn_keeps_visible_text_when_the_model_produced_some() {
    let messages = project_messages(
        &[NormalizedTurn::Assistant {
            text: Some("Listing files.".into()),
            tool_uses: vec![ToolUse {
                tool_use_id: "call_1".into(),
                tool_kind: ToolKind::RunShellCommand,
                input: json!({"command": "ls -la"}),
            }],
            reasoning: None,
            message_ids: vec![],
        }],
        "system",
    );

    assert_eq!(
        assistant_message(&messages)
            .get("content")
            .and_then(|c| c.as_str()),
        Some("Listing files.")
    );
}

#[test]
fn plain_assistant_turn_always_carries_content() {
    let messages = project_messages(
        &[NormalizedTurn::Assistant {
            text: Some("Hello!".into()),
            tool_uses: vec![],
            reasoning: None,
            message_ids: vec![],
        }],
        "system",
    );

    assert_eq!(
        assistant_message(&messages)
            .get("content")
            .and_then(|c| c.as_str()),
        Some("Hello!")
    );
}

#[test]
fn tool_results_follow_their_calling_assistant_turn() {
    let messages = project_messages(
        &[
            NormalizedTurn::User {
                text: Some("查看当前源码的架构及作用".into()),
                tool_results: vec![],
            },
            tool_call_only_assistant(),
            NormalizedTurn::User {
                text: None,
                tool_results: vec![ToolResult {
                    tool_use_id: "call_1".into(),
                    tool_kind: ToolKind::RunShellCommand,
                    content: "total 0".into(),
                    is_error: false,
                }],
            },
        ],
        "system",
    );

    assert_eq!(
        messages
            .iter()
            .map(|m| m.get("role").and_then(|r| r.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>(),
        vec!["system", "user", "assistant", "tool"]
    );
    assert_eq!(
        messages[3].get("tool_call_id").and_then(|v| v.as_str()),
        Some("call_1")
    );
}

#[test]
fn shape_summary_flags_absent_and_null_content() {
    let messages = project_messages(&[tool_call_only_assistant()], "system");

    let shape = message_shape(&messages);

    assert!(shape.contains("assistant("), "{shape}");
    assert!(
        shape.contains("no-content"),
        "tool-call turn has no content key: {shape}"
    );
    assert!(shape.contains("tool_calls[1]:call_1"), "{shape}");
}
