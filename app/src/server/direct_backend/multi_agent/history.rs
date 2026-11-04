//! Per-conversation transcript retention for the stateless direct LLM backend.
//!
//! Upstream, `warp-server` owns the task transcript, so a follow-up request
//! carries the current task's `ToolCall` messages plus their `ToolCallResult`
//! inputs, but not the earlier turns of the conversation. Provider calls are
//! stateless HTTP requests, so replaying that alone opens the conversation on
//! an `assistant` message with no user turn — Ark/GLM answer with an opaque
//! `InvalidParameter` 400 and Anthropic rejects the shape outright. The
//! transcript is kept here and spliced onto the request's batch, whose tool
//! items repeat what the cached turns already sent.

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
/// Floor for a stale tool result once the transcript has to fit
/// [`MAX_TRANSCRIPT_CHARS`]. Shrinking a payload keeps a call paired with its
/// result; evicting the step instead makes the model re-run the whole command,
/// because the client re-announces every tool call the task has made.
const MIN_TOOL_RESULT_CHARS: usize = 1_200;

/// Stand-in prompt for a transcript that opens mid-step, e.g. because the
/// owning user turn was dropped by [`MAX_TURNS_PER_CONVERSATION`].
const DROPPED_HISTORY_NOTE: &str = "[System: earlier turns of this conversation are not available to the \
     direct LLM backend. Answer from the messages shown below; do not apologize for the missing context.]";

static TRANSCRIPTS: OnceLock<Mutex<Transcripts>> = OnceLock::new();

#[derive(Default)]
struct Transcripts {
    by_conversation: HashMap<String, Vec<NormalizedTurn>>,
    /// Call ids already put on the wire per conversation. The client ships the
    /// task's whole tool batch on every follow-up, so "already sent" has to
    /// outlive the retention window — otherwise a step the char budget evicted
    /// comes back as a call with no result.
    sent_call_ids: HashMap<String, HashSet<String>>,
    /// Task-message ids whose prose (text/reasoning) already went on the wire.
    /// Like [`Self::sent_call_ids`], this outlives the retention window; the
    /// follow-up batch replays the current task's messages, and replayed prose
    /// is what made the model parrot its own preamble forever.
    sent_message_ids: HashMap<String, HashSet<String>>,
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
        let known = self
            .sent_call_ids
            .get(conversation_id)
            .cloned()
            .unwrap_or_default();
        let known_msgs = self
            .sent_message_ids
            .get(conversation_id)
            .cloned()
            .unwrap_or_default();
        let overlap = align(&cached, &incoming);
        log::trace!(
            "DirectBackend history merge: conv={conversation_id} cached={} incoming={} overlap={} cached_keys={:?} incoming_keys={:?}",
            cached.len(),
            incoming.len(),
            overlap,
            key_digest(&cached),
            key_digest(&incoming),
        );
        let mut merged = cached[..cached.len() - overlap].to_vec();
        merged.extend(incoming);
        // The first `cached.len()` merged turns are the transcript we already
        // put on the wire, including an aligned re-send of the tail. Only what
        // comes after that is new.
        let sent_len = cached.len();

        let duplicates = dedupe_replayed_items(&mut merged, sent_len, &known, &known_msgs);
        if duplicates > 0 {
            log::warn!(
                "DirectBackend history: {conversation_id} dropped {duplicates} replayed item(s)"
            );
        }
        let elided = compact_stale_tool_results(&mut merged, sent_len);
        if elided > 0 {
            log::warn!("DirectBackend history: {conversation_id} elided {elided} stale result(s)");
        }
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
        self.remember_sent_ids(conversation_id, &merged);

        self.by_conversation
            .insert(conversation_id.to_owned(), merged.clone());
        self.touch(conversation_id);
        self.evict();
        merged
    }

    fn remember_sent_ids(&mut self, conversation_id: &str, turns: &[NormalizedTurn]) {
        let sent = self
            .sent_call_ids
            .entry(conversation_id.to_owned())
            .or_default();
        let sent_msgs = self
            .sent_message_ids
            .entry(conversation_id.to_owned())
            .or_default();
        for turn in turns {
            if let NormalizedTurn::Assistant {
                tool_uses,
                message_ids,
                ..
            } = turn
            {
                for call in tool_uses {
                    sent.insert(call.tool_use_id.clone());
                }
                for id in message_ids {
                    sent_msgs.insert(id.clone());
                }
            }
        }
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
                    self.sent_call_ids.remove(&stale);
                    self.sent_message_ids.remove(&stale);
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

/// Turn-key digests for the trace-level merge alignment log.
fn key_digest(turns: &[NormalizedTurn]) -> Vec<u64> {
    turns.iter().map(turn_key).collect()
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
            text,
            tool_uses,
            message_ids,
            ..
        } => {
            1u8.hash(&mut hasher);
            if tool_uses.is_empty() {
                text.as_deref().unwrap_or_default().hash(&mut hasher);
            } else {
                for tool_use in tool_uses {
                    tool_use.tool_use_id.hash(&mut hasher);
                }
            }
            for id in message_ids {
                id.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

/// Drops tool calls, tool results, and prose that an earlier turn of the
/// transcript already carries, then drops turns left with nothing to send.
///
/// The client ships the whole current task transcript on every follow-up
/// (`task_context.tasks[*].messages`), so each request's batch is a superset of
/// what was already put on the wire. [`align`] cannot see that: replayed steps
/// arrive without their interleaved tool-result turns, so turn chains never
/// match. `known` adds the call ids and `known_msgs` the task-message ids of
/// steps already sent (including ones the retention budget has since evicted);
/// a replayed assistant turn with only known ids loses its text/reasoning —
/// leaving it in is what fed the model copies of its own preamble until it
/// started repeating them verbatim. Turns before `sent_len` are the transcript
/// already put on the wire and always define the baseline.
fn dedupe_replayed_items(
    turns: &mut Vec<NormalizedTurn>,
    sent_len: usize,
    known: &HashSet<String>,
    known_msgs: &HashSet<String>,
) -> usize {
    let mut called: HashSet<String> = HashSet::new();
    let mut answered: HashSet<String> = HashSet::new();
    let mut seen_msgs: HashSet<&str> = HashSet::new();
    let mut dropped = 0usize;
    for (idx, turn) in turns.iter_mut().enumerate() {
        let replayed = idx >= sent_len;
        match turn {
            NormalizedTurn::Assistant {
                text,
                tool_uses,
                reasoning,
                message_ids,
            } => {
                let before = tool_uses.len();
                tool_uses.retain(|call| {
                    let sent_earlier = known.contains(&call.tool_use_id);
                    called.insert(call.tool_use_id.clone()) && !(replayed && sent_earlier)
                });
                dropped += before - tool_uses.len();
                if replayed
                    && !message_ids.is_empty()
                    && message_ids
                        .iter()
                        .all(|id| known_msgs.contains(id) || seen_msgs.contains(id.as_str()))
                {
                    if text.take().is_some() {
                        dropped += 1;
                    }
                    if reasoning.take().is_some() {
                        dropped += 1;
                    }
                }
                seen_msgs.extend(message_ids.iter().map(String::as_str));
            }
            NormalizedTurn::User { tool_results, .. } => {
                let before = tool_results.len();
                tool_results.retain(|result| answered.insert(result.tool_use_id.clone()));
                dropped += before - tool_results.len();
            }
        }
    }
    turns.retain(is_populated);
    dropped
}

/// A turn with neither prose nor tool traffic renders as an empty message,
/// which providers reject.
fn is_populated(turn: &NormalizedTurn) -> bool {
    match turn {
        NormalizedTurn::User { text, tool_results } => text.is_some() || !tool_results.is_empty(),
        NormalizedTurn::Assistant {
            text,
            tool_uses,
            reasoning,
            ..
        } => text.is_some() || !tool_uses.is_empty() || reasoning.is_some(),
    }
}

/// Elides tool-result payloads until the transcript fits
/// [`MAX_TRANSCRIPT_CHARS`], keeping every call paired with its result. Stale
/// turns shrink first; the request's fresh results are only touched when that
/// is not enough, so a payload larger than the whole budget is elided rather
/// than evicting the step that owns it.
fn compact_stale_tool_results(turns: &mut [NormalizedTurn], keep_from: usize) -> usize {
    if transcript_chars(turns) <= MAX_TRANSCRIPT_CHARS {
        return 0;
    }
    let mut elided = compact_until(turns, keep_from);
    let all = turns.len();
    if transcript_chars(turns) > MAX_TRANSCRIPT_CHARS {
        elided += compact_until(turns, all);
    }
    elided
}

fn compact_until(turns: &mut [NormalizedTurn], limit: usize) -> usize {
    let mut elided = 0usize;
    for idx in 0..limit.min(turns.len()) {
        if transcript_chars(turns) <= MAX_TRANSCRIPT_CHARS {
            break;
        }
        if let NormalizedTurn::User { tool_results, .. } = &mut turns[idx] {
            for result in tool_results.iter_mut() {
                if let Some(shrunk) = elide_middle(&result.content, MIN_TOOL_RESULT_CHARS) {
                    result.content = shrunk;
                    elided += 1;
                }
            }
        }
    }
    elided
}

/// Keeps the head and tail of `text` and replaces the middle, or returns `None`
/// when `text` is already within `budget` characters.
fn elide_middle(text: &str, budget: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= budget {
        return None;
    }
    let head_len = budget / 2;
    let tail_len = budget - head_len;
    let head: String = chars[..head_len].iter().collect();
    let tail: String = chars[chars.len() - tail_len..].iter().collect();
    Some(format!(
        "{head}\n[... {} characters elided by the direct backend's transcript budget ...]\n{tail}",
        chars.len() - budget,
    ))
}

/// Drops the oldest turns until both the turn count and the text budget fit,
/// taking a tool call with the turns that answer it so the provider never sees
/// a call with no result after it.
fn enforce_budget(turns: &mut Vec<NormalizedTurn>) -> usize {
    let mut dropped = 0usize;
    while turns.len() > 1
        && (turns.len() > MAX_TURNS_PER_CONVERSATION
            || transcript_chars(turns) > MAX_TRANSCRIPT_CHARS)
    {
        let mut take = 1usize;
        if let Some(NormalizedTurn::Assistant { tool_uses, .. }) = turns.first()
            && !tool_uses.is_empty()
        {
            while take < turns.len()
                && matches!(
                    &turns[take],
                    NormalizedTurn::User { text: None, tool_results }
                        if !tool_results.is_empty()
                )
            {
                take += 1;
            }
        }
        turns.drain(..take);
        dropped += take;
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

    turns.retain(is_populated);
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
