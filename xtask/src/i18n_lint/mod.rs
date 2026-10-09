use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, ValueEnum};
use walkdir::WalkDir;

mod allowlist;
mod parity;
mod report;
mod sites;
mod visitor;

#[cfg(test)]
mod tests;

pub use allowlist::Allowlist;
pub use report::{Severity, Violation};
pub use sites::SiteTable;

/// Scan a single Rust source string. Helper exposed for the xtask self-tests
/// (`xtask/tests/i18n_lint_test.rs`) so they can build assertions without
/// touching the filesystem.
pub fn scan_source(
    rel_path: &Path,
    source: &str,
    sites: &SiteTable,
    allowlist: &Allowlist,
) -> Vec<Violation> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut violations = visitor::scan_file(rel_path, source, &file, sites, allowlist);
    sort_violations(&mut violations);
    violations
}

fn sort_violations(violations: &mut [Violation]) {
    violations.sort_by(|a, b| {
        (a.file.as_path(), a.line, a.column, &a.literal).cmp(&(
            b.file.as_path(),
            b.line,
            b.column,
            &b.literal,
        ))
    });
}

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long, value_enum, default_value_t = Mode::Warning)]
    mode: Mode,

    /// Verify that en and zh-CN bundles share the same key set per file.
    #[arg(long)]
    check_parity: bool,

    /// Override the allowlist path (defaults to crates/warp_i18n/lint_allowlist.toml).
    #[arg(long)]
    allowlist: Option<PathBuf>,

    /// Override the bundles root (defaults to crates/warp_i18n/bundles).
    #[arg(long)]
    bundles: Option<PathBuf>,
}

#[derive(Copy, Clone, ValueEnum)]
pub enum Mode {
    Warning,
    Hard,
}

pub fn run(args: Args, repo_root: &Path) -> Result<ExitCode> {
    if args.check_parity {
        let bundles = args
            .bundles
            .clone()
            .unwrap_or_else(|| repo_root.join("crates/warp_i18n/bundles"));
        let mismatches = parity::check(&bundles)?;
        if !mismatches.is_empty() {
            for m in &mismatches {
                eprintln!("{m}");
            }
            eprintln!("\n{} parity mismatch(es)", mismatches.len());
            return Ok(ExitCode::from(1));
        }
        eprintln!("bundle parity OK");
        return Ok(ExitCode::SUCCESS);
    }

    let allowlist_path = args
        .allowlist
        .clone()
        .unwrap_or_else(|| repo_root.join("crates/warp_i18n/lint_allowlist.toml"));
    let allowlist = if allowlist_path.exists() {
        Allowlist::load(&allowlist_path)
            .with_context(|| format!("loading allowlist {}", allowlist_path.display()))?
    } else {
        Allowlist::empty()
    };

    let sites = sites::SiteTable::default();
    let violations = scan_repo(repo_root, &sites, &allowlist)?;

    let (hard, soft): (Vec<_>, Vec<_>) = violations
        .iter()
        .partition(|v| v.severity == Severity::Hard);

    report::render(&violations);

    let fail = !hard.is_empty() || (matches!(args.mode, Mode::Hard) && !soft.is_empty());
    eprintln!(
        "\n{} hard violation(s), {} soft warning(s) (mode: {:?})",
        hard.len(),
        soft.len(),
        match args.mode {
            Mode::Warning => "warning",
            Mode::Hard => "hard",
        }
    );

    Ok(if fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

pub(crate) fn scan_repo(
    root: &Path,
    sites: &sites::SiteTable,
    allowlist: &Allowlist,
) -> Result<Vec<Violation>> {
    let scan_roots = ["app", "crates"];
    let mut paths = Vec::new();
    for sub in scan_roots {
        let dir = root.join(sub);
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            if sites.is_excluded(&path, root) {
                continue;
            }
            paths.push(path);
        }
    }
    paths.sort();

    let mut violations = Vec::new();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(_) => continue,
        };
        violations.extend(visitor::scan_file(&rel, &src, &file, sites, allowlist));
    }
    sort_violations(&mut violations);
    Ok(violations)
}
