use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use xtask::i18n_lint;

#[derive(Parser)]
#[command(name = "xtask", about = "Repo automation tasks for warp")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(name = "check-i18n", about = "Lint UI strings for i18n coverage")]
    CheckI18n(i18n_lint::Args),
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::CheckI18n(args) => i18n_lint::run(args, &repo_root()),
    }
}

fn repo_root() -> PathBuf {
    // Cargo sets CARGO_MANIFEST_DIR to xtask/; repo root is parent.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.clone())
}
