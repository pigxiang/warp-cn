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
        message_ids: vec![],
    }
}

fn replayed_step(msg_id: &str, text: &str, call_ids: &[&str]) -> NormalizedTurn {
    NormalizedTurn::Assistant {
        text: Some(text.to_owned()),
        tool_uses: call_ids
            .iter()
            .map(|id| ToolUse {
                tool_use_id: (*id).to_owned(),
                tool_kind: ToolKind::RunShellCommand,
                input: json!({ "command": "ls" }),
            })
            .collect(),
        reasoning: Some(format!("thinking about {text}")),
        message_ids: vec![msg_id.to_owned()],
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

fn call_ids(turns: &[NormalizedTurn]) -> Vec<String> {
    let mut ids = Vec::new();
    for turn in turns {
        if let NormalizedTurn::Assistant { tool_uses, .. } = turn {
            ids.extend(tool_uses.iter().map(|call| call.tool_use_id.clone()));
        }
    }
    ids
}

fn result_ids(turns: &[NormalizedTurn]) -> Vec<String> {
    let mut ids = Vec::new();
    for turn in turns {
        if let NormalizedTurn::User { tool_results, .. } = turn {
            ids.extend(tool_results.iter().map(|result| result.tool_use_id.clone()));
        }
    }
    ids
}

fn user_result_with_content(id: &str, content: &str) -> NormalizedTurn {
    NormalizedTurn::User {
        text: None,
        tool_results: vec![ToolResult {
            tool_use_id: id.to_owned(),
            tool_kind: ToolKind::RunShellCommand,
            content: content.to_owned(),
            is_error: false,
        }],
    }
}

/// Call ids with no `tool` result immediately after their turn — the shape
/// providers reject or that forces a fabricated retry stub.
fn unpaired_call_ids(turns: &[NormalizedTurn]) -> Vec<String> {
    let mut unpaired = Vec::new();
    let mut idx = 0usize;
    while idx < turns.len() {
        if let NormalizedTurn::Assistant { tool_uses, .. } = &turns[idx] {
            let called: Vec<String> = tool_uses
                .iter()
                .map(|call| call.tool_use_id.clone())
                .collect();
            let mut answered: Vec<String> = Vec::new();
            let mut next = idx + 1;
            while let Some(NormalizedTurn::User {
                text: None,
                tool_results,
            }) = turns.get(next)
            {
                answered.extend(tool_results.iter().map(|result| result.tool_use_id.clone()));
                next += 1;
            }
            unpaired.extend(called.into_iter().filter(|id| !answered.contains(id)));
            idx = next;
        } else {
            idx += 1;
        }
    }
    unpaired
}

/// Assistant prose of each turn, in order.
fn assistant_texts(turns: &[NormalizedTurn]) -> Vec<String> {
    turns
        .iter()
        .filter_map(|turn| match turn {
            NormalizedTurn::Assistant {
                text: Some(text), ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn replayed_step_prose_is_not_sent_twice() {
    let mut store = Transcripts::default();
    let first = store.merge("conv-a", vec![user_text("map the repo")]);
    assert_eq!(first.len(), 1);

    let second = store.merge(
        "conv-a",
        vec![
            replayed_step("m1", "I will list the tree.", &["c1"]),
            user_results(&["c1"]),
        ],
    );
    assert_eq!(assistant_texts(&second), vec!["I will list the tree."]);

    // The next follow-up replays step 1 alongside step 2. Step 1's prose and
    // calls have already gone on the wire, so only step 2 may add prose —
    // otherwise the model sees its own preamble multiply and starts repeating
    // it every turn.
    let third = store.merge(
        "conv-a",
        vec![
            replayed_step("m1", "I will list the tree.", &["c1"]),
            replayed_step("m2", "Now let me read the scripts.", &["c2"]),
            user_results(&["c2"]),
        ],
    );
    assert_eq!(
        assistant_texts(&third),
        vec!["I will list the tree.", "Now let me read the scripts."]
    );
    assert_eq!(call_ids(&third), vec!["c1", "c2"]);
    assert_eq!(result_ids(&third), vec!["c1", "c2"]);
    assert!(unpaired_call_ids(&third).is_empty());
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
fn a_batch_that_repeats_earlier_steps_contributes_only_the_new_step() {
    let mut store = Transcripts::default();
    store.merge("conv-g", vec![user_text("go")]);
    store.merge(
        "conv-g",
        vec![assistant_calls(&["c1"]), user_results(&["c1"])],
    );

    let merged = store.merge(
        "conv-g",
        vec![
            assistant_calls(&["c1", "c2"]),
            user_results(&["c2", "c1"]),
            user_text("next"),
        ],
    );
    assert_eq!(
        merged.iter().map(role_of).collect::<Vec<_>>(),
        vec!["user", "assistant", "user", "assistant", "user", "user"]
    );
    assert_eq!(call_ids(&merged), vec!["c1", "c2"]);
    assert_eq!(result_ids(&merged), vec!["c1", "c2"]);
}

#[test]
fn cumulative_batches_keep_the_transcript_linear() {
    let mut store = Transcripts::default();
    store.merge("conv-h", vec![user_text("go")]);

    let ids: Vec<String> = (1..=10).map(|i| format!("c{i}")).collect();
    let mut merged = Vec::new();
    for step in 1..=10 {
        let batch: Vec<&str> = ids[..step].iter().map(String::as_str).collect();
        merged = store.merge(
            "conv-h",
            vec![assistant_calls(&batch), user_results(&batch)],
        );
    }

    assert_eq!(merged.len(), 1 + 2 * ids.len());
    assert_eq!(call_ids(&merged).len(), ids.len());
    assert_eq!(result_ids(&merged).len(), ids.len());
}

#[test]
fn oversized_results_are_elided_and_keep_their_call_paired() {
    let mut store = Transcripts::default();
    store.merge("conv-j", vec![user_text("go")]);

    let mut merged = Vec::new();
    for step in 1..=8 {
        let id = format!("c{step}");
        merged = store.merge(
            "conv-j",
            vec![
                assistant_calls(&[id.as_str()]),
                user_result_with_content(&id, &"x".repeat(60_000)),
            ],
        );
    }

    assert!(transcript_chars(&merged) <= MAX_TRANSCRIPT_CHARS);
    assert_eq!(
        call_ids(&merged),
        (1..=8).map(|i| format!("c{i}")).collect::<Vec<_>>()
    );
    assert!(
        unpaired_call_ids(&merged).is_empty(),
        "eliding must not orphan a call",
    );
}

#[test]
fn a_result_larger_than_the_whole_budget_is_elided_not_evicted() {
    let mut store = Transcripts::default();
    store.merge("conv-k", vec![user_text("go")]);

    let merged = store.merge(
        "conv-k",
        vec![
            assistant_calls(&["c1"]),
            user_result_with_content("c1", &"y".repeat(300_000)),
        ],
    );
    assert!(call_ids(&merged).contains(&"c1".to_owned()));
    assert!(transcript_chars(&merged) <= MAX_TRANSCRIPT_CHARS);
    assert!(unpaired_call_ids(&merged).is_empty());
}

#[test]
fn re_announced_calls_stay_dropped_after_the_turn_cap_evicts_a_step() {
    let mut store = Transcripts::default();
    store.merge("conv-l", vec![user_text("go")]);

    let mut merged = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for step in 1..=MAX_TURNS_PER_CONVERSATION / 2 + 3 {
        let id = format!("c{step}");
        ids.push(id.clone());
        merged = store.merge(
            "conv-l",
            vec![
                assistant_calls(&[id.as_str()]),
                user_results(&[id.as_str()]),
            ],
        );
    }
    // The synthetic lead note [`ensure_user_lead`] may add one turn on top.
    assert!(
        merged.len() <= MAX_TURNS_PER_CONVERSATION + 1,
        "the turn cap must have evicted the oldest steps, len={}",
        merged.len()
    );
    let all: Vec<&str> = ids.iter().map(String::as_str).collect();
    let after = store.merge("conv-l", vec![assistant_calls(&all), user_results(&all)]);

    let before_ids = call_ids(&merged);
    let after_ids = call_ids(&after);
    assert!(
        after_ids.iter().all(|id| before_ids.contains(id)),
        "a re-announced call must not come back: {after_ids:?}",
    );
    assert!(after_ids.len() <= before_ids.len());
    assert_eq!(after_ids.last().map(String::as_str), Some("c63"));
    assert!(unpaired_call_ids(&after).is_empty());
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
