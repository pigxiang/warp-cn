//! `api::Request` → provider-neutral chat history.
//!
//! Walks `task_context.tasks[*].messages` chronologically plus the current
//! `input.user_inputs`, emitting `Vec<NormalizedTurn>` where each turn groups
//! all consecutive atoms produced by the same actor (model response or
//! user/tool side). Without grouping, Anthropic rejects "roles must alternate"
//! and OpenAI rejects unbatched `tool_calls` arrays.
//!
//! Anything outside our Tier-1/2 tool surface downgrades the history to "drop
//! everything except the last user text" — providers refuse chats with
//! dangling tool_use blocks and we can't fabricate missing tool_results.
//!
//! warp-server renders attached blocks, selected text and environment context into
//! the prompt server-side; [`context`] does it here, and the render rides in front
//! of the user's own text.

use serde_json::Value;
use warp_multi_agent_api as api;

use super::tool_schema::{
    ToolKind, message_result_to_text, proto_to_history, request_result_to_text,
};

/// One tool call inside an assistant turn.
#[derive(Debug, Clone)]
pub struct ToolUse {
    pub tool_use_id: String,
    pub tool_kind: ToolKind,
    pub input: Value,
}

/// One tool result inside a user turn.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub tool_kind: ToolKind,
    pub content: String,
    pub is_error: bool,
}

/// Grouped turn: one user or assistant block, possibly mixing text + tool
/// uses/results, ready to be projected to a single provider message.
#[derive(Debug, Clone)]
pub enum NormalizedTurn {
    User {
        text: Option<String>,
        tool_results: Vec<ToolResult>,
    },
    Assistant {
        text: Option<String>,
        tool_uses: Vec<ToolUse>,
        /// Chain-of-thought from reasoning models. DeepSeek-R1 / o1 require
        /// the prior turn's `reasoning_content` to be echoed back in the
        /// next request, otherwise they reject with HTTP 400. Other
        /// providers (Anthropic / Gemini) ignore this.
        reasoning: Option<String>,
        /// Task-message ids that produced `text`/`reasoning`. The task batch
        /// replays every step of the current task on each follow-up request,
        /// so [`super::history`] drops the prose of a replayed turn once these
        /// ids have already gone on the wire — the transcript must carry each
        /// assistant sentence to the provider exactly once.
        message_ids: Vec<String>,
    },
}

#[derive(Debug)]
pub struct DecodedRequest {
    pub conversation_id: String,
    /// Existing task ID to AddMessagesToTask onto. None means we should emit
    /// a fresh CreateTask. Reusing an existing ID is required for multi-turn
    /// conversations because the server-backed root task can't be re-upgraded.
    pub existing_task_id: Option<String>,
    pub turns: Vec<NormalizedTurn>,
    /// `false` if any unsupported tool/result was encountered. Caller dropped
    /// the history down to "last user text"; UX surfaces a system note.
    #[allow(dead_code)]
    pub history_compatible: bool,
    /// `true` if at least one user input was extracted.
    pub has_user_input: bool,
    /// Size of the attached-context render that was folded into this request's user turn. The
    /// direct backend has no server-side prompt assembly, so this is the only handle on whether
    /// a user's attached block actually reached the provider.
    pub attached_context_chars: usize,
    /// MCP context (servers + their resources/tools) carried through to each
    /// driver so the system prompt can advertise valid `server_id` values
    /// for the `read_mcp_resource` / `call_mcp_tool` Tier-3 tools.
    pub mcp_context: Option<api::request::McpContext>,
    /// `Settings.ModelConfig.base` — the LLM ID the user chose in the
    /// `/MODEL` picker. When present this **overrides** the configured
    /// `DirectBackendConfig` `model_id` so the picker selection actually
    /// reaches the provider, instead of silently falling back to the
    /// per-provider default (`gpt-4o-mini` / `claude-sonnet-4-6` / …).
    pub base_model: Option<String>,
}

#[derive(Debug, Clone)]
enum Atom {
    UserText(String),
    UserToolResult(ToolResult),
    AssistantText {
        message_id: String,
        request_id: String,
        text: String,
    },
    AssistantReasoning {
        message_id: String,
        request_id: String,
        text: String,
    },
    AssistantToolUse {
        message_id: String,
        request_id: String,
        tool: ToolUse,
    },
}

pub fn decode(req: &api::Request) -> DecodedRequest {
    let conversation_id = req
        .metadata
        .as_ref()
        .map(|m| m.conversation_id.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    let existing_task_id = req
        .task_context
        .as_ref()
        .and_then(|tc| tc.tasks.first())
        .map(|t| t.id.clone())
        .filter(|id| !id.is_empty());

    let mut atoms = Vec::new();
    let mut compat = true;
    let mut has_user_input = false;

    if let Some(ctx) = req.task_context.as_ref() {
        for task in &ctx.tasks {
            log::trace!(
                "DirectBackend decode: task={} messages={}",
                task.id,
                task.messages.len()
            );
            for (i, msg) in task.messages.iter().enumerate() {
                log::trace!(
                    "DirectBackend decode: task_msg[{i}] id={} req={} {}",
                    msg.id,
                    msg.request_id,
                    describe_message(msg.message.as_ref()),
                );
                project_history_message(msg, &mut atoms, &mut compat, &mut has_user_input);
            }
        }
    }
    if let Some(input) = req.input.as_ref()
        && let Some(api::request::input::Type::UserInputs(inputs)) = input.r#type.as_ref()
    {
        for (i, ui) in inputs.inputs.iter().enumerate() {
            log::trace!(
                "DirectBackend decode: input[{i}] {}",
                describe_user_input(ui.input.as_ref()),
            );
        }
    }

    let mut attached_context = req
        .input
        .as_ref()
        .and_then(super::context::render_for_input);
    let attached_chars = attached_context
        .as_ref()
        .map_or(0, |rendered| rendered.chars().count());

    if let Some(input) = req.input.as_ref()
        && let Some(api::request::input::Type::UserInputs(inputs)) = input.r#type.as_ref()
    {
        for ui in &inputs.inputs {
            if let Some(inner) = ui.input.as_ref() {
                project_user_input(
                    inner,
                    &mut attached_context,
                    &mut atoms,
                    &mut compat,
                    &mut has_user_input,
                );
            }
        }
    }
    // A tool-result-only input has no user turn to carry the request's context, and the turn
    // that was answered already went out with it on the first request of the step.
    let attached_context_chars = match attached_context {
        Some(dropped) => {
            let dropped_chars = dropped.chars().count();
            log::debug!(
                "DirectBackend: no user query to carry {dropped_chars} chars of attached context"
            );
            0
        }
        None => attached_chars,
    };

    let turns = if compat {
        coalesce(atoms)
    } else {
        // History truncated; surface a synthetic system note so the model
        // (and the client log) understand context is missing.
        let last_user = atoms
            .into_iter()
            .rev()
            .find(|a| matches!(a, Atom::UserText(_)));
        match last_user {
            Some(Atom::UserText(t)) => vec![NormalizedTurn::User {
                text: Some(format!(
                    "[System: previous conversation history was dropped because it referenced \
                     tools this build doesn't support; only your latest message is visible.]\n\n{t}"
                )),
                tool_results: vec![],
            }],
            _ => vec![],
        }
    };

    let base_model = req
        .settings
        .as_ref()
        .and_then(|s| s.model_config.as_ref())
        .map(|mc| mc.base.trim().to_owned())
        .filter(|s| !s.is_empty());

    DecodedRequest {
        conversation_id,
        existing_task_id,
        turns,
        history_compatible: compat,
        has_user_input,
        attached_context_chars,
        mcp_context: req.mcp_context.clone(),
        base_model,
    }
}

fn coalesce(atoms: Vec<Atom>) -> Vec<NormalizedTurn> {
    fn is_user_atom(atom: &Atom) -> bool {
        matches!(atom, Atom::UserText(_) | Atom::UserToolResult(_))
    }
    fn request_of(atom: &Atom) -> Option<&str> {
        match atom {
            Atom::AssistantText { request_id, .. }
            | Atom::AssistantReasoning { request_id, .. }
            | Atom::AssistantToolUse { request_id, .. } => Some(request_id.as_str()),
            _ => None,
        }
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < atoms.len() {
        let is_user = is_user_atom(&atoms[i]);
        // Task batches replay the current task's steps from previous requests.
        // Assistant atoms must not fuse across response boundaries: each
        // response's messages share one `request_id`, and the turn shape that
        // reaches [`super::history::merge`] has to identify one step, whose
        // message ids then dedupe the replay.
        let step = request_of(&atoms[i]).map(str::to_owned);

        let mut text_parts: Vec<String> = Vec::new();
        let mut reasoning_parts: Vec<String> = Vec::new();
        let mut tool_uses: Vec<ToolUse> = Vec::new();
        let mut tool_results: Vec<ToolResult> = Vec::new();
        let mut message_ids: Vec<String> = Vec::new();

        while i < atoms.len() && is_user_atom(&atoms[i]) == is_user {
            if !is_user {
                let same_step = match (step.as_deref(), request_of(&atoms[i])) {
                    (Some(step), Some(request)) => step == request,
                    _ => true,
                };
                if !same_step {
                    break;
                }
            }
            match atoms[i].clone() {
                Atom::UserText(t) => text_parts.push(t),
                Atom::UserToolResult(r) => tool_results.push(r),
                Atom::AssistantText {
                    message_id, text, ..
                } => {
                    text_parts.push(text);
                    push_new_id(&mut message_ids, message_id);
                }
                Atom::AssistantReasoning {
                    message_id, text, ..
                } => {
                    reasoning_parts.push(text);
                    push_new_id(&mut message_ids, message_id);
                }
                Atom::AssistantToolUse {
                    message_id, tool, ..
                } => {
                    tool_uses.push(tool);
                    push_new_id(&mut message_ids, message_id);
                }
            }
            i += 1;
        }

        let text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n\n"))
        };
        let reasoning = if reasoning_parts.is_empty() {
            None
        } else {
            Some(reasoning_parts.join("\n\n"))
        };

        if is_user {
            // Skip empty user turns (no text, no results) — they emit nothing
            // useful and providers reject empty messages.
            if text.is_some() || !tool_results.is_empty() {
                out.push(NormalizedTurn::User { text, tool_results });
            }
        } else if text.is_some() || !tool_uses.is_empty() || reasoning.is_some() {
            out.push(NormalizedTurn::Assistant {
                text,
                tool_uses,
                reasoning,
                message_ids,
            });
        }
    }
    out
}

fn push_new_id(ids: &mut Vec<String>, id: String) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

/// Attached context rides in front of the user's own text so the model reads it as part of the
/// question it was attached to. Taken once per request.
fn prefix_query(attached_context: &mut Option<String>, query: &str) -> String {
    match attached_context.take() {
        Some(context) => format!("{context}\n\n{query}"),
        None => query.to_owned(),
    }
}

fn project_history_message(
    msg: &api::Message,
    out: &mut Vec<Atom>,
    compat: &mut bool,
    has_user_input: &mut bool,
) {
    use api::message::Message as M;
    match msg.message.as_ref() {
        Some(M::UserQuery(uq)) => {
            if !uq.query.is_empty() {
                let mut attached = super::context::render_for_history_query(uq);
                out.push(Atom::UserText(prefix_query(&mut attached, &uq.query)));
                *has_user_input = true;
            }
        }
        Some(M::AgentOutput(ao)) => {
            if !ao.text.is_empty() {
                out.push(Atom::AssistantText {
                    message_id: msg.id.clone(),
                    request_id: msg.request_id.clone(),
                    text: ao.text.clone(),
                });
            }
        }
        Some(M::AgentReasoning(ar)) => {
            if !ar.reasoning.is_empty() {
                out.push(Atom::AssistantReasoning {
                    message_id: msg.id.clone(),
                    request_id: msg.request_id.clone(),
                    text: ar.reasoning.clone(),
                });
            }
        }
        Some(M::ToolCall(tc)) => match tc.tool.as_ref() {
            Some(tool) => match proto_to_history(tool) {
                Some((tool_kind, input)) => out.push(Atom::AssistantToolUse {
                    message_id: msg.id.clone(),
                    request_id: msg.request_id.clone(),
                    tool: ToolUse {
                        tool_use_id: tc.tool_call_id.clone(),
                        tool_kind,
                        input,
                    },
                }),
                None => *compat = false,
            },
            None => *compat = false,
        },
        Some(M::ToolCallResult(tcr)) => match tcr.result.as_ref() {
            Some(r) => match message_result_to_text(r) {
                Some((tool_kind, content)) => {
                    let is_error = content.starts_with("ERROR:");
                    out.push(Atom::UserToolResult(ToolResult {
                        tool_use_id: tcr.tool_call_id.clone(),
                        tool_kind,
                        content,
                        is_error,
                    }))
                }
                None => *compat = false,
            },
            None => *compat = false,
        },
        _ => {}
    }
}

fn project_user_input(
    input: &api::request::input::user_inputs::user_input::Input,
    attached_context: &mut Option<String>,
    out: &mut Vec<Atom>,
    compat: &mut bool,
    has_user_input: &mut bool,
) {
    use api::request::input::user_inputs::user_input::Input as I;
    match input {
        I::UserQuery(uq) => {
            if !uq.query.is_empty() {
                out.push(Atom::UserText(prefix_query(attached_context, &uq.query)));
                *has_user_input = true;
            }
        }
        I::CliAgentUserQuery(cli) => {
            if let Some(uq) = cli.user_query.as_ref()
                && !uq.query.is_empty()
            {
                out.push(Atom::UserText(prefix_query(attached_context, &uq.query)));
                *has_user_input = true;
            }
        }
        I::ToolCallResult(tcr) => match tcr.result.as_ref() {
            Some(r) => match request_result_to_text(r) {
                Some((tool_kind, content)) => {
                    let is_error = content.starts_with("ERROR:");
                    out.push(Atom::UserToolResult(ToolResult {
                        tool_use_id: tcr.tool_call_id.clone(),
                        tool_kind,
                        content,
                        is_error,
                    }));
                    // Tool results are the user-side input of the next agent
                    // turn — without this, multi-turn loops bail at `has_user_input`.
                    *has_user_input = true;
                }
                None => *compat = false,
            },
            None => *compat = false,
        },
        _ => *compat = false,
    }
}

/// Trace-level structural dump of the incoming request, for transcript-
/// duplication field diagnosis.
fn describe_message(msg: Option<&api::message::Message>) -> String {
    use api::message::Message as M;
    match msg {
        Some(M::UserQuery(uq)) => format!("UserQuery chars={}", uq.query.chars().count()),
        Some(M::AgentOutput(ao)) => format!("AgentOutput chars={}", ao.text.chars().count()),
        Some(M::AgentReasoning(ar)) => {
            format!("AgentReasoning chars={}", ar.reasoning.chars().count())
        }
        Some(M::ToolCall(tc)) => format!("ToolCall id={}", tc.tool_call_id),
        Some(M::ToolCallResult(tcr)) => format!("ToolCallResult id={}", tcr.tool_call_id),
        Some(_) => "other".to_owned(),
        None => "empty".to_owned(),
    }
}

/// Trace-level structural dump of the incoming request, for transcript-
/// duplication field diagnosis.
fn describe_user_input(
    input: Option<&api::request::input::user_inputs::user_input::Input>,
) -> String {
    use api::request::input::user_inputs::user_input::Input as I;
    match input {
        Some(I::UserQuery(uq)) => format!("UserQuery chars={}", uq.query.chars().count()),
        Some(I::CliAgentUserQuery(cli)) => format!(
            "CliAgentUserQuery chars={}",
            cli.user_query
                .as_ref()
                .map_or(0, |uq| uq.query.chars().count())
        ),
        Some(I::ToolCallResult(tcr)) => format!("ToolCallResult id={}", tcr.tool_call_id),
        Some(_) => "other".to_owned(),
        None => "empty".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(text: &str) -> api::Message {
        api::Message {
            id: "m".into(),
            task_id: "t".into(),
            request_id: "r".into(),
            timestamp: None,
            server_message_data: String::new(),
            citations: vec![],
            fetched_memories: vec![],
            message: Some(api::message::Message::UserQuery(api::message::UserQuery {
                query: text.into(),
                ..Default::default()
            })),
        }
    }

    fn agent_msg(text: &str) -> api::Message {
        api::Message {
            id: "m".into(),
            task_id: "t".into(),
            request_id: "r".into(),
            timestamp: None,
            server_message_data: String::new(),
            citations: vec![],
            fetched_memories: vec![],
            message: Some(api::message::Message::AgentOutput(
                api::message::AgentOutput { text: text.into() },
            )),
        }
    }

    fn agent_tool_msg(id: &str) -> api::Message {
        let rf = api::message::tool_call::ReadFiles {
            files: vec![api::message::tool_call::read_files::File {
                name: "x".into(),
                line_ranges: vec![],
            }],
        };
        api::Message {
            id: "m".into(),
            task_id: "t".into(),
            request_id: "r".into(),
            timestamp: None,
            server_message_data: String::new(),
            citations: vec![],
            fetched_memories: vec![],
            message: Some(api::message::Message::ToolCall(api::message::ToolCall {
                tool_call_id: id.into(),
                tool: Some(api::message::tool_call::Tool::ReadFiles(rf)),
            })),
        }
    }

    #[test]
    fn empty_request_yields_no_turns() {
        let d = decode(&api::Request::default());
        assert!(d.turns.is_empty());
        assert!(!d.has_user_input);
        assert!(d.history_compatible);
        assert!(d.existing_task_id.is_none());
    }

    #[test]
    fn user_query_groups_with_assistant_text() {
        let req = api::Request {
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "task-1".into(),
                    messages: vec![user_msg("hello"), agent_msg("hi back")],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert_eq!(d.existing_task_id.as_deref(), Some("task-1"));
        assert!(d.has_user_input);
        assert_eq!(d.turns.len(), 2);
        assert!(matches!(d.turns[0], NormalizedTurn::User { .. }));
        assert!(matches!(d.turns[1], NormalizedTurn::Assistant { .. }));
    }

    #[test]
    fn assistant_text_and_tool_use_coalesce() {
        let req = api::Request {
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "t".into(),
                    messages: vec![
                        user_msg("read it"),
                        agent_msg("here goes"),
                        agent_tool_msg("tc1"),
                    ],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert_eq!(d.turns.len(), 2);
        if let NormalizedTurn::Assistant {
            text, tool_uses, ..
        } = &d.turns[1]
        {
            assert_eq!(text.as_deref(), Some("here goes"));
            assert_eq!(tool_uses.len(), 1);
            assert_eq!(tool_uses[0].tool_use_id, "tc1");
        } else {
            panic!("expected grouped Assistant turn");
        }
    }

    #[test]
    fn assistant_turns_split_at_response_boundaries() {
        fn step_message(
            id: &str,
            request_id: &str,
            message: api::message::Message,
        ) -> api::Message {
            api::Message {
                id: id.into(),
                task_id: "t".into(),
                request_id: request_id.into(),
                timestamp: None,
                server_message_data: String::new(),
                citations: vec![],
                fetched_memories: vec![],
                message: Some(message),
            }
        }
        let tool = |call_id: &str| {
            api::message::Message::ToolCall(api::message::ToolCall {
                tool_call_id: call_id.into(),
                tool: Some(api::message::tool_call::Tool::ReadFiles(
                    api::message::tool_call::ReadFiles {
                        files: vec![api::message::tool_call::read_files::File {
                            name: "x".into(),
                            line_ranges: vec![],
                        }],
                    },
                )),
            })
        };
        // A follow-up batch replays step 1's messages before step 2's; without
        // the response boundary split their prose would fuse into one turn and
        // step 1's text could never be deduped out of the transcript.
        let req = api::Request {
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "task-1".into(),
                    messages: vec![
                        step_message(
                            "ao-1",
                            "r-1",
                            api::message::Message::AgentOutput(api::message::AgentOutput {
                                text: "I will map the tree.".into(),
                            }),
                        ),
                        step_message("tc-1", "r-1", tool("call-1")),
                        step_message(
                            "ao-2",
                            "r-2",
                            api::message::Message::AgentOutput(api::message::AgentOutput {
                                text: "Now I will read scripts.".into(),
                            }),
                        ),
                    ],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert_eq!(d.turns.len(), 2);
        let (first, second) = (&d.turns[0], &d.turns[1]);
        if let NormalizedTurn::Assistant {
            text,
            tool_uses,
            message_ids,
            ..
        } = first
        {
            assert_eq!(text.as_deref(), Some("I will map the tree."));
            assert_eq!(tool_uses.len(), 1);
            assert_eq!(message_ids, &["ao-1".to_owned(), "tc-1".to_owned()]);
        } else {
            panic!("expected first Assistant turn");
        }
        if let NormalizedTurn::Assistant {
            text, tool_uses, ..
        } = second
        {
            assert_eq!(text.as_deref(), Some("Now I will read scripts."));
            assert!(tool_uses.is_empty());
        } else {
            panic!("expected second Assistant turn");
        }
    }

    #[test]
    fn unsupported_tool_call_triggers_degrade_with_system_note() {
        let req = api::Request {
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "t".into(),
                    messages: vec![
                        user_msg("first"),
                        api::Message {
                            id: "m".into(),
                            task_id: "t".into(),
                            request_id: "r".into(),
                            timestamp: None,
                            server_message_data: String::new(),
                            citations: vec![],
                            fetched_memories: vec![],
                            message: Some(api::message::Message::ToolCall(
                                api::message::ToolCall {
                                    tool_call_id: "tc".into(),
                                    tool: Some(api::message::tool_call::Tool::SearchCodebase(
                                        api::message::tool_call::SearchCodebase::default(),
                                    )),
                                },
                            )),
                        },
                        user_msg("after search"),
                    ],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert!(!d.history_compatible);
        assert_eq!(d.turns.len(), 1);
        if let NormalizedTurn::User { text, .. } = &d.turns[0] {
            let t = text.as_deref().unwrap();
            assert!(t.starts_with("[System:"));
            assert!(t.ends_with("after search"));
        } else {
            panic!("expected User turn");
        }
    }

    #[test]
    fn current_user_input_appended() {
        let req = api::Request {
            input: Some(api::request::Input {
                context: None,
                r#type: Some(api::request::input::Type::UserInputs(
                    api::request::input::UserInputs {
                        inputs: vec![api::request::input::user_inputs::UserInput {
                            input: Some(
                                api::request::input::user_inputs::user_input::Input::UserQuery(
                                    api::request::input::UserQuery {
                                        query: "show me lib.rs".into(),
                                        ..Default::default()
                                    },
                                ),
                            ),
                        }],
                    },
                )),
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert!(d.has_user_input);
        assert_eq!(d.turns.len(), 1);
        if let NormalizedTurn::User { text, .. } = &d.turns[0] {
            assert_eq!(text.as_deref(), Some("show me lib.rs"));
        }
    }

    #[test]
    #[allow(deprecated)]
    fn attached_block_rides_in_front_of_the_user_query() {
        let req = api::Request {
            input: Some(query_input(
                "why did this fail?",
                Some(api::InputContext {
                    executed_shell_commands: vec![api::ExecutedShellCommand {
                        command: "cargo build".into(),
                        output: "error: could not compile".into(),
                        exit_code: 101,
                        command_id: "b1".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            )),
            ..Default::default()
        };
        let d = decode(&req);
        assert_eq!(d.turns.len(), 1);
        assert!(d.attached_context_chars > 0);
        if let NormalizedTurn::User { text, .. } = &d.turns[0] {
            let text = text.as_deref().unwrap();
            assert!(text.starts_with("<user-attached-context>"));
            assert!(text.contains("command: cargo build"));
            assert!(text.contains("error: could not compile"));
            assert!(text.ends_with("why did this fail?"));
        } else {
            panic!("expected User turn");
        }
    }

    #[test]
    #[allow(deprecated)]
    fn attached_context_is_not_repeated_on_a_tool_result_turn() {
        let attached = api::InputContext {
            directory: Some(api::input_context::Directory {
                pwd: "/repo".into(),
                home: String::new(),
                pwd_file_symbols_indexed: false,
            }),
            executed_shell_commands: vec![api::ExecutedShellCommand {
                command: "cargo build".into(),
                output: "error: could not compile".into(),
                exit_code: 101,
                command_id: "b1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let req = api::Request {
            input: Some(api::request::Input {
                context: Some(attached),
                r#type: Some(api::request::input::Type::UserInputs(
                    api::request::input::UserInputs {
                        inputs: vec![api::request::input::user_inputs::UserInput {
                            input: Some(
                                api::request::input::user_inputs::user_input::Input::ToolCallResult(
                                    api::request::input::ToolCallResult {
                                        tool_call_id: "t1".into(),
                                        result: Some(
                                            api::request::input::tool_call_result::Result::ReadFiles(
                                                api::ReadFilesResult { result: None },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                        }],
                    },
                )),
            }),
            ..Default::default()
        };
        let d = decode(&req);
        assert_eq!(
            d.attached_context_chars, 0,
            "dropped renders are not counted"
        );
        let last = d.turns.last().expect("one tool-result turn");
        if let NormalizedTurn::User { text, tool_results } = last {
            assert!(text.is_none(), "no environment context on a tool turn");
            assert_eq!(tool_results.len(), 1);
        } else {
            panic!("expected last turn to be User");
        }
    }

    #[test]
    #[allow(deprecated)]
    fn replayed_history_query_keeps_the_block_attached_to_it() {
        let history = api::Message {
            id: "m".into(),
            task_id: "t".into(),
            request_id: "r".into(),
            timestamp: None,
            server_message_data: String::new(),
            citations: vec![],
            fetched_memories: vec![],
            message: Some(api::message::Message::UserQuery(api::message::UserQuery {
                query: "why did that fail?".into(),
                context: Some(api::InputContext {
                    executed_shell_commands: vec![api::ExecutedShellCommand {
                        command: "make".into(),
                        output: "gcc: error".into(),
                        exit_code: 2,
                        command_id: "b2".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            })),
        };
        let req = api::Request {
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "task-1".into(),
                    messages: vec![history],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        if let NormalizedTurn::User { text, .. } = &d.turns[0] {
            let text = text.as_deref().unwrap();
            assert!(text.contains("gcc: error"));
            assert!(text.ends_with("why did that fail?"));
        } else {
            panic!("expected User turn");
        }
    }

    #[test]
    fn multiple_tool_results_group_into_one_user_turn() {
        let req = api::Request {
            input: Some(api::request::Input {
                context: None,
                r#type: Some(api::request::input::Type::UserInputs(
                    api::request::input::UserInputs {
                        inputs: vec![
                            api::request::input::user_inputs::UserInput {
                                input: Some(
                                    api::request::input::user_inputs::user_input::Input::ToolCallResult(
                                        api::request::input::ToolCallResult {
                                            tool_call_id: "t1".into(),
                                            result: Some(api::request::input::tool_call_result::Result::ReadFiles(
                                                api::ReadFilesResult { result: None },
                                            )),
                                        },
                                    ),
                                ),
                            },
                            api::request::input::user_inputs::UserInput {
                                input: Some(
                                    api::request::input::user_inputs::user_input::Input::ToolCallResult(
                                        api::request::input::ToolCallResult {
                                            tool_call_id: "t2".into(),
                                            result: Some(api::request::input::tool_call_result::Result::ReadFiles(
                                                api::ReadFilesResult { result: None },
                                            )),
                                        },
                                    ),
                                ),
                            },
                        ],
                    },
                )),
            }),
            task_context: Some(api::request::TaskContext {
                tasks: vec![api::Task {
                    id: "t".into(),
                    messages: vec![user_msg("seed")],
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let d = decode(&req);
        // Last turn should be one user turn carrying TWO tool results.
        let last = d.turns.last().unwrap();
        if let NormalizedTurn::User { tool_results, .. } = last {
            assert_eq!(tool_results.len(), 2);
            assert_eq!(tool_results[0].tool_use_id, "t1");
            assert_eq!(tool_results[1].tool_use_id, "t2");
        } else {
            panic!("expected last turn to be User");
        }
    }

    fn query_input(query: &str, context: Option<api::InputContext>) -> api::request::Input {
        api::request::Input {
            context,
            r#type: Some(api::request::input::Type::UserInputs(
                api::request::input::UserInputs {
                    inputs: vec![api::request::input::user_inputs::UserInput {
                        input: Some(
                            api::request::input::user_inputs::user_input::Input::UserQuery(
                                api::request::input::UserQuery {
                                    query: query.into(),
                                    ..Default::default()
                                },
                            ),
                        ),
                    }],
                },
            )),
        }
    }
}
