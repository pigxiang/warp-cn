use cloud_object_models::EnvVarValue;
use itertools::Itertools as _;
use warp_util::path::ShellFamily;

use async_channel::Sender;
pub use warp_terminal::shell::{shell_escape_single_quotes, shell_quote_arg};

use crate::terminal::shell::ShellType;

use crate::terminal::model::session::command_executor::{
    InBandCommand, InBandCommandCancelledEvent,
};

/// Set of events sent by command executors.
pub enum ExecutorCommandEvent {
    /// The command should be executed.
    ExecuteCommand {
        command: InBandCommand,
        /// A Sender that can be used to signal that the command has been cancelled.
        /// Lets us unblock the command in the executor.
        cancel_tx: Sender<InBandCommandCancelledEvent>,
    },
    /// The command identified by `id` should be cancelled.
    CancelCommand { id: String },
}

/// Serializes environment-variable pairs into a shell-specific assignment prefix so
/// in-band commands (SSH/WSL) observe the session's environment.
pub fn serialize_variables_for_shell<'s, I: IntoIterator<Item = (&'s str, &'s EnvVarValue)>>(
    pairs: I,
    shell_type: ShellType,
) -> String {
    match shell_type {
        // Warp doesn't support newlines in fish so we can't use env syntax
        ShellType::Fish => {
            serialize_variables_internal(pairs, "set -x ", " ", ";", " ", shell_type.into())
        }
        ShellType::Bash | ShellType::Zsh => {
            serialize_variables_internal(pairs, "", "=", "", " ", shell_type.into())
        }
        ShellType::PowerShell => {
            serialize_variables_internal(pairs, "$env:", " = ", ";", " ", shell_type.into())
        }
    }
}

// Prefix — what's prepended to each variable
// Separator — what separates the variable name from the value
// Postfix — what's appended to the end of each variable
// Delimiter — what separates one variable from the next one
// set -x var_name var_value;   set -x name2 value2;
// ------     -             -   -
//   ^        ^             ^   ^
// prefix  separator   postfix  delimiter (in this case 4 spaces, usually one space or newline)
fn serialize_variables_internal<'s, I: IntoIterator<Item = (&'s str, &'s EnvVarValue)>>(
    pairs: I,
    prefix: &str,
    separator: &str,
    postfix: &str,
    delimiter: &str,
    shell_family: ShellFamily,
) -> String {
    pairs
        .into_iter()
        .map(|(name, value)| {
            format!(
                "{}{}{}{}{}",
                prefix,
                shell_family.escape(name),
                separator,
                get_init_command_for_env_var(value, shell_family),
                postfix
            )
        })
        .collect_vec()
        .join(delimiter)
}

fn get_init_command_for_env_var(value: &EnvVarValue, shell_family: ShellFamily) -> String {
    match value {
        EnvVarValue::Constant(val) => match shell_family {
            ShellFamily::Posix => shell_family.escape(val).into_owned(),
            ShellFamily::PowerShell => format!("'{}'", val.replace("'", "''")),
        },
        EnvVarValue::Command(cmd) => format!("$({})", cmd.command),
        EnvVarValue::Secret(secret) => {
            format!("$({})", secret.get_secret_extraction_command(shell_family))
        }
    }
}
