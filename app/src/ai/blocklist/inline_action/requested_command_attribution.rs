//! Module to attribute AI-generated requested commands
//! to known documents (e.g. Warp Drive objects).

use markdown_parser::{FormattedTextLine, parse_markdown};
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent::AIAgentCitation;
use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::workflow_command_parser::command_matches_workflow;
use crate::notebooks::CloudNotebookModel;
use crate::terminal::shell::ShellType;

/// Returns true iff the `command` is directly copied from the `document`.
pub(crate) fn is_command_copied_from_document(
    command: &str,
    document: &AIAgentCitation,
    ctx: &AppContext,
) -> bool {
    let command = command.trim();

    match document {
        AIAgentCitation::WarpDriveObject { uid } => {
            is_command_copied_from_warp_drive_object(command, uid, ctx)
        }
        _ => false,
    }
}

/// Returns true iff the `command` is directly copied from the
/// Warp Drive object identified by `object_uid`.
fn is_command_copied_from_warp_drive_object(
    command: &str,
    object_uid: &str,
    ctx: &AppContext,
) -> bool {
    if let Some(workflow) = CloudModel::as_ref(ctx).get_workflow_by_uid(object_uid) {
        command_matches_workflow(command, &workflow.model().data)
    } else if let Some(notebook) = CloudModel::as_ref(ctx).get_notebook_by_uid(object_uid) {
        is_command_copied_from_notebook(command, notebook.model())
    } else {
        false
    }
}

/// Returns true iff the `command` was copied directly from one of the
/// notebook's code blocks.
fn is_command_copied_from_notebook(command: &str, notebook: &CloudNotebookModel) -> bool {
    let Ok(md) = parse_markdown(notebook.data.as_str()) else {
        return false;
    };

    for line in md.lines {
        if let FormattedTextLine::CodeBlock(code) = line
            && command == code.code.trim()
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
#[path = "requested_command_attribution_tests.rs"]
mod tests;
