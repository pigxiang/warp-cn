//! Per-conversation transcript retention for the stateless direct LLM backend.
//!
//! Upstream, `warp-server` owns the task transcript, so a follow-up request
//! only carries the delta: the assistant's `ToolCall` messages plus the
//! matching `ToolCallResult` inputs. Provider calls are stateless HTTP
//! requests, so replaying that delta alone opens the conversation on an
//! `assistant` message with no user turn — Ark/GLM answers with an opaque
//! `InvalidParameter` 400 and Anthropic rejects the shape outright. The
//! transcript is kept here and each delta is spliced onto the cached prefix.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use parking_lot::Mutex;

use super::decode::NormalizedTurn;

const MAX_CONVERSATIONS: usize = 32;
const MAX_TURNS_PER_CONVERSATION: usize = 120;
/// Ceiling on replayed transcript text. Tool results carry whole file and
/// shell-output dumps, and upstream context-window management lives in
/// `warp-server`, so nothing else bounds what this path would send.
const MAX_TRANSCRIPT_CHARS: usize = 240_000;

/// Stand-in prompt for a transcript that opens mid-step, e.g. because the
/// owning user turn was dropped by [`MAX_TURNS_PER_CONVERSATION`].
const DROPPED_HISTORY_NOTE: &str = "[System: earlier turns of this conversation are not available to the \
     direct LLM backend. Answer from the messages shown below; do not apologize for the missing context.]";

static TRANSCRIPTS: OnceLock<Mutex<Transcripts>> = OnceLock::new();

#[derive(Default)]
struct Transcripts {
    by_conversation: HashMap<String, Vec<NormalizedTurn>>,
    /// Oldest first; evicted from the front once too many conversations are
    /// retained. Time-based expiry is avoided because this path also compiles
    /// for wasm, where a monotonic clock is not guaranteed.
    lru: VecDeque<String>,
}

impl Transcripts {
    /// Splices `incoming` onto the cached prefix for `conversation_id` and
    /// returns the transcript to put on the wire, which also becomes the new
    /// cached prefix.
    fn merge(
        &mut self,
        conversation_id: &str,
        incoming: Vec<NormalizedTurn>,
    ) -> Vec<NormalizedTurn> {
        let cached = self
            .by_conversation
            .get(conversation_id)
            .cloned()
            .unwrap_or_default();
        let overlap = align(&cached, &incoming);
        let mut merged = cached[..cached.len() - overlap].to_vec();
        merged.extend(incoming);

        let trimmed = enforce_budget(&mut merged);
        if trimmed > 0 {
            log::warn!(
                "DirectBackend history: trimmed {trimmed} oldest turn(s) of {conversation_id}"
            );
        }
        let orphaned = strip_orphaned_tool_results(&mut merged);
        if orphaned > 0 {
            log::warn!("DirectBackend history: stripped {orphaned} orphaned tool result(s)");
        }
        ensure_user_lead(&mut merged);

        self.by_conversation
            .insert(conversation_id.to_owned(), merged.clone());
        self.touch(conversation_id);
        self.evict();
        merged
    }

    fn touch(&mut self, conversation_id: &str) {
        self.lru.retain(|id| id != conversation_id);
        self.lru.push_back(conversation_id.to_owned());
    }

    fn evict(&mut self) {
        while self.lru.len() > MAX_CONVERSATIONS {
            match self.lru.pop_front() {
                Some(stale) => {
                    self.by_conversation.remove(&stale);
                }
                None => break,
            }
        }
    }
}

/// Returns the transcript to put on the wire for `conversation_id`, taking and
/// updating the process-wide cache.
pub(super) fn merge(conversation_id: &str, incoming: Vec<NormalizedTurn>) -> Vec<NormalizedTurn> {
    let lock = TRANSCRIPTS.get_or_init(|| Mutex::new(Transcripts::default()));
    let mut store = lock.lock();
    store.merge(conversation_id, incoming)
}

/// Largest `k` such that the last `k` cached turns are the same steps as the
/// first `k` incoming turns.
fn align(cached: &[NormalizedTurn], incoming: &[NormalizedTurn]) -> usize {
    for k in (1..=cached.len().min(incoming.len())).rev() {
        let aligned = cached[cached.len() - k..]
            .iter()
            .zip(incoming[..k].iter())
            .all(|(cached, incoming)| turn_key(cached) == turn_key(incoming));
        if aligned {
            return k;
        }
    }
    0
}

/// Identity of a turn for alignment. Tool-call ids identify a step; one call's
/// result text is rendered separately by the history projection and the input
/// projection, so content is deliberately left out of the key.
fn turn_key(turn: &NormalizedTurn) -> u64 {
    let mut hasher = DefaultHasher::new();
    match turn {
        NormalizedTurn::User { text, tool_results } => {
            0u8.hash(&mut hasher);
            if tool_results.is_empty() {
                text.as_deref().unwrap_or_default().hash(&mut hasher);
            } else {
                for result in tool_results {
                    result.tool_use_id.hash(&mut hasher);
                }
            }
        }
        NormalizedTurn::Assistant {
            text, tool_uses, ..
        } => {
            1u8.hash(&mut hasher);
            if tool_uses.is_empty() {
                text.as_deref().unwrap_or_default().hash(&mut hasher);
            } else {
                for tool_use in tool_uses {
                    tool_use.tool_use_id.hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

/// Drops the oldest turns until both the turn count and the text budget fit.
fn enforce_budget(turns: &mut Vec<NormalizedTurn>) -> usize {
    let mut dropped = 0usize;
    while turns.len() > 1
        && (turns.len() > MAX_TURNS_PER_CONVERSATION
            || transcript_chars(turns) > MAX_TRANSCRIPT_CHARS)
    {
        turns.drain(..1);
        dropped += 1;
    }
    dropped
}

fn transcript_chars(turns: &[NormalizedTurn]) -> usize {
    let len = |text: &Option<String>| text.as_ref().map_or(0, String::len);
    turns
        .iter()
        .map(|turn| match turn {
            NormalizedTurn::User { text, tool_results } => {
                len(text)
                    + tool_results
                        .iter()
                        .map(|result| result.content.len())
                        .sum::<usize>()
            }
            NormalizedTurn::Assistant {
                text, reasoning, ..
            } => len(text) + len(reasoning),
        })
        .sum()
}

fn strip_orphaned_tool_results(turns: &mut Vec<NormalizedTurn>) -> usize {
    let called: HashSet<String> = turns
        .iter()
        .filter_map(|turn| match turn {
            NormalizedTurn::Assistant { tool_uses, .. } => Some(tool_uses.iter()),
            NormalizedTurn::User { .. } => None,
        })
        .flatten()
        .map(|tool_use| tool_use.tool_use_id.clone())
        .collect();

    let mut stripped = 0usize;
    for turn in turns.iter_mut() {
        if let NormalizedTurn::User { tool_results, .. } = turn {
            let before = tool_results.len();
            tool_results.retain(|result| called.contains(&result.tool_use_id));
            stripped += before - tool_results.len();
        }
    }

    turns.retain(|turn| match turn {
        NormalizedTurn::User { text, tool_results } => text.is_some() || !tool_results.is_empty(),
        NormalizedTurn::Assistant {
            text,
            tool_uses,
            reasoning,
        } => text.is_some() || !tool_uses.is_empty() || reasoning.is_some(),
    });
    stripped
}

fn ensure_user_lead(turns: &mut Vec<NormalizedTurn>) {
    if matches!(
        turns.first(),
        Some(NormalizedTurn::User { text: Some(_), .. })
    ) {
        return;
    }
    turns.insert(
        0,
        NormalizedTurn::User {
            text: Some(DROPPED_HISTORY_NOTE.to_owned()),
            tool_results: vec![],
        },
    );
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
