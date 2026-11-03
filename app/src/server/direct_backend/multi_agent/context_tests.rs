use std::collections::HashMap;

use super::*;

fn attached_block(command: &str, output: &str, exit_code: i32) -> api::ExecutedShellCommand {
    api::ExecutedShellCommand {
        command: command.into(),
        output: output.into(),
        exit_code,
        command_id: "cmd-1".into(),
        ..Default::default()
    }
}

fn input_with_context(ctx: api::InputContext) -> api::request::Input {
    api::request::Input {
        context: Some(ctx),
        ..Default::default()
    }
}

#[test]
fn empty_request_context_renders_nothing() {
    assert!(render_for_input(&api::request::Input::default()).is_none());
}

#[test]
#[allow(deprecated)]
fn attached_block_renders_command_output_and_exit_code() {
    let rendered = render_for_input(&input_with_context(api::InputContext {
        executed_shell_commands: vec![attached_block("cargo test", "test result: FAILED.", 101)],
        ..Default::default()
    }))
    .expect("an attached block renders");

    assert!(rendered.starts_with("<user-attached-context>"));
    assert!(rendered.ends_with("</user-attached-context>"));
    assert!(rendered.contains("Terminal block attached by the user:"));
    assert!(rendered.contains("command: cargo test"));
    assert!(rendered.contains("exit code: 101"));
    assert!(rendered.contains("test result: FAILED."));
}

#[test]
#[allow(deprecated)]
fn auto_attached_block_is_labelled_as_such() {
    let mut block = attached_block("ls", "src", 0);
    block.is_auto_attached = true;
    let rendered = render_for_input(&input_with_context(api::InputContext {
        executed_shell_commands: vec![block],
        ..Default::default()
    }))
    .expect("an attached block renders");

    assert!(
        rendered
            .contains("Terminal block attached by the user (auto-attached from the agent view):")
    );
}

#[test]
fn selected_text_renders_verbatim() {
    let rendered = render_for_input(&input_with_context(api::InputContext {
        selected_text: vec![api::input_context::SelectedText {
            text: "panic: index out of bounds".into(),
        }],
        ..Default::default()
    }))
    .expect("selected text renders");

    assert!(rendered.contains("Text the user selected in the terminal:"));
    assert!(rendered.contains("<selection>\npanic: index out of bounds\n</selection>"));
}

#[test]
fn environment_renders_directory_git_shell_and_os() {
    let rendered = render_for_input(&input_with_context(api::InputContext {
        directory: Some(api::input_context::Directory {
            pwd: "/repo".into(),
            home: "/home/u".into(),
            pwd_file_symbols_indexed: false,
        }),
        shell: Some(api::input_context::Shell {
            name: "zsh".into(),
            version: "5.9".into(),
        }),
        operating_system: Some(api::input_context::OperatingSystem {
            platform: "MacOS".into(),
            distribution: String::new(),
        }),
        git: Some(api::input_context::Git {
            head: "deadbeef".into(),
            branch: "main".into(),
            ..Default::default()
        }),
        current_time: Some(prost_types::Timestamp {
            seconds: 1_762_000_000,
            nanos: 0,
        }),
        ..Default::default()
    }))
    .expect("environment context renders");

    assert!(rendered.contains("Environment:"));
    assert!(rendered.contains("- working directory: /repo"));
    assert!(rendered.contains("- home directory: /home/u"));
    assert!(rendered.contains("- git branch: main"));
    assert!(rendered.contains("- git head: deadbeef"));
    assert!(rendered.contains("- shell: zsh 5.9"));
    assert!(rendered.contains("- operating system: MacOS"));
    assert!(rendered.contains("- current time: "));
}

#[test]
fn context_without_renderable_items_renders_nothing() {
    let ctx = api::InputContext {
        codebases: vec![api::input_context::Codebase {
            name: "repo".into(),
            path: "/repo".into(),
        }],
        images: vec![api::input_context::Image {
            data: vec![1, 2, 3],
            mime_type: "image/png".into(),
        }],
        ..Default::default()
    };
    assert!(render_for_input(&input_with_context(ctx)).is_none());
}

#[test]
#[allow(deprecated)]
fn referenced_attachments_render_and_keep_marker_order() {
    let block = api::Attachment {
        value: Some(api::attachment::Value::ExecutedShellCommand(
            attached_block("ls", "src", 0),
        )),
    };
    let plan = api::Attachment {
        value: Some(api::attachment::Value::DocumentContent(
            api::DocumentContent {
                document_id: "plan-1".into(),
                content: "1. ship it".into(),
                ..Default::default()
            },
        )),
    };
    let input = api::request::Input {
        r#type: Some(api::request::input::Type::UserInputs(
            api::request::input::UserInputs {
                inputs: vec![api::request::input::user_inputs::UserInput {
                    input: Some(
                        api::request::input::user_inputs::user_input::Input::UserQuery(
                            api::request::input::UserQuery {
                                query: "explain <block:abc> against <plan:plan-1>".into(),
                                referenced_attachments: HashMap::from([
                                    ("<plan:plan-1>".to_owned(), plan),
                                    ("<block:abc>".to_owned(), block),
                                ]),
                                ..Default::default()
                            },
                        ),
                    ),
                }],
            },
        )),
        ..Default::default()
    };

    let rendered = render_for_input(&input).expect("attachments render");
    assert!(rendered.contains("referenced in their question as `<block:abc>`"));
    assert!(rendered.contains("command: ls"));
    assert!(rendered.contains("1. ship it"));
    let block_at = rendered.find("<block:abc>").expect("block marker");
    let plan_at = rendered.find("<plan:plan-1>").expect("plan marker");
    assert!(
        block_at < plan_at,
        "attachments must render in marker order, not HashMap order:\n{rendered}"
    );
}

#[test]
fn oversized_attachment_is_elided() {
    let huge = "x".repeat(MAX_ITEM_CHARS + 500);
    let rendered = render_for_input(&input_with_context(api::InputContext {
        selected_text: vec![api::input_context::SelectedText { text: huge }],
        ..Default::default()
    }))
    .expect("selected text renders");

    assert!(rendered.contains("characters of this attachment elided by the direct backend"));
    assert!(rendered.chars().count() < MAX_ITEM_CHARS + 1_000);
}

#[test]
#[allow(deprecated)]
fn history_query_renders_its_block_but_not_its_environment() {
    let query = api::message::UserQuery {
        query: "why did that fail?".into(),
        context: Some(api::InputContext {
            directory: Some(api::input_context::Directory {
                pwd: "/repo".into(),
                home: String::new(),
                pwd_file_symbols_indexed: false,
            }),
            executed_shell_commands: vec![attached_block("make", "gcc: error", 2)],
            ..Default::default()
        }),
        ..Default::default()
    };

    let rendered = render_for_history_query(&query).expect("a replayed block renders");
    assert!(rendered.contains("command: make"));
    assert!(!rendered.contains("working directory"));
}
