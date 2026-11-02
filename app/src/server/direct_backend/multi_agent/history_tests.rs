use serde_json::json;

use super::*;
use crate::server::direct_backend::multi_agent::decode::{NormalizedTurn, ToolResult, ToolUse};
use crate::server::direct_backend::multi_agent::tool_schema::ToolKind;

fn user_text(text: &str) -> NormalizedTurn {
    NormalizedTurn::User {
        text: Some(text.to_owned()),
        tool_results: vec![],
    }
}

fn assistant_calls(ids: &[&str]) -> NormalizedTurn {
    NormalizedTurn::Assistant {
        text: None,
        tool_uses: ids
            .iter()
            .map(|id| ToolUse {
                tool_use_id: (*id).to_owned(),
                tool_kind: ToolKind::RunShellCommand,
                input: json!({ "command": "ls" }),
            })
            .collect(),
        reasoning: None,
    }
}

fn user_results(ids: &[&str]) -> NormalizedTurn {
    NormalizedTurn::User {
        text: None,
        tool_results: ids
            .iter()
            .map(|id| ToolResult {
                tool_use_id: (*id).to_owned(),
                tool_kind: ToolKind::RunShellCommand,
                content: "done".to_owned(),
                is_error: false,
            })
            .collect(),
    }
}

fn leading_text(turns: &[NormalizedTurn]) -> Option<String> {
    match turns.first() {
        Some(NormalizedTurn::User { text, .. }) => text.clone(),
        _ => None,
    }
}

fn role_of(turn: &NormalizedTurn) -> &'static str {
    match turn {
        NormalizedTurn::User { .. } => "user",
        NormalizedTurn::Assistant { .. } => "assistant",
    }
}

#[test]
fn first_user_turn_survives_the_tool_followup() {
    let mut store = Transcripts::default();
    let first = store.merge("conv-a", vec![user_text("list the files")]);
    assert_eq!(first.len(), 1);

    let followup = store.merge(
        "conv-a",
        vec![assistant_calls(&["c1", "c2"]), user_results(&["c1", "c2"])],
    );
    assert_eq!(
        followup.iter().map(role_of).collect::<Vec<_>>(),
        vec!["user", "assistant", "user"]
    );
    assert_eq!(leading_text(&followup).as_deref(), Some("list the files"));
}

#[test]
fn replayed_overlap_is_not_sent_twice() {
    let mut store = Transcripts::default();
    store.merge("conv-b", vec![user_text("go")]);
    store.merge(
        "conv-b",
        vec![assistant_calls(&["c1"]), user_results(&["c1"])],
    );

    let replayed = store.merge(
        "conv-b",
        vec![
            assistant_calls(&["c1"]),
            user_results(&["c1"]),
            assistant_calls(&["c2"]),
            user_results(&["c2"]),
        ],
    );
    assert_eq!(replayed.len(), 5);
    assert_eq!(leading_text(&replayed).as_deref(), Some("go"));
}

#[test]
fn a_transcript_that_opens_mid_step_gets_a_synthetic_user_turn() {
    let mut store = Transcripts::default();
    let merged = store.merge(
        "conv-c",
        vec![assistant_calls(&["c1"]), user_results(&["c1"])],
    );
    assert_eq!(role_of(merged.first().unwrap()), "user");
    assert!(leading_text(&merged).unwrap().contains("not available"));
    assert_eq!(merged.len(), 3);
}

#[test]
fn tool_results_without_a_matching_call_are_dropped() {
    let mut store = Transcripts::default();
    let merged = store.merge("conv-d", vec![user_results(&["ghost"]), user_text("next")]);
    assert_eq!(merged.len(), 1);
    assert_eq!(leading_text(&merged).as_deref(), Some("next"));
}

#[test]
fn trimming_keeps_a_user_lead_and_a_complete_tool_step() {
    let mut store = Transcripts::default();
    let seed = (0..MAX_TURNS_PER_CONVERSATION)
        .map(|i| user_text(&format!("q{i}")))
        .collect();
    store.merge("conv-e", seed);

    let overflow = store.merge(
        "conv-e",
        vec![assistant_calls(&["keep"]), user_results(&["keep"])],
    );
    assert_eq!(overflow.len(), MAX_TURNS_PER_CONVERSATION);
    assert!(leading_text(&overflow).is_some());
    assert_eq!(role_of(overflow.last().unwrap()), "user");
}

#[test]
fn the_text_budget_evicts_oldest_turns_first() {
    let mut store = Transcripts::default();
    let big = "x".repeat(30_000);
    let mut seed: Vec<NormalizedTurn> = (0..9).map(|_| user_text(&big)).collect();
    seed.push(user_text("last"));

    let merged = store.merge("conv-f", seed);
    assert_eq!(merged.len(), 8);
    assert_eq!(transcript_chars(&merged), 7 * 30_000 + "last".len());
    assert_eq!(leading_text(&merged).as_deref(), Some(big.as_str()));
    assert_eq!(role_of(merged.last().unwrap()), "user");
}

#[test]
fn conversations_beyond_the_cap_are_evicted() {
    let mut store = Transcripts::default();
    store.merge("evicted", vec![user_text("seed")]);
    for i in 0..MAX_CONVERSATIONS + 1 {
        store.merge(format!("filler-{i}").as_str(), vec![user_text("hi")]);
    }

    let after = store.merge("evicted", vec![user_text("again")]);
    assert_eq!(after.len(), 1);
    assert_eq!(leading_text(&after).as_deref(), Some("again"));
}
