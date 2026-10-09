use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobMatcher};
use serde::Deserialize;

/// Schema mirrors design.md D21.
#[derive(Debug, Deserialize)]
struct Raw {
    schema_version: u32,
    #[serde(default)]
    phase5_baseline_count: u32,
    #[serde(default, rename = "entries")]
    entries: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    file_glob: String,
    callsite: String,
    literal: String,
    reason: String,
    added_phase: u8,
    owner: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Entry {
    pub file_matcher: GlobMatcher,
    pub callsite: String,
    pub literal: String,
    pub key: EntryKey,
}

/// Stable key for set membership: order-independent identity per design D21.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryKey {
    pub file_glob: String,
    pub callsite: String,
    pub literal: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Allowlist {
    pub schema_version: u32,
    pub phase5_baseline_count: u32,
    entries: Vec<Entry>,
    keys: BTreeSet<EntryKey>,
}

impl Allowlist {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            phase5_baseline_count: 0,
            entries: Vec::new(),
            keys: BTreeSet::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).context("parsing allowlist toml")?;
        if raw.schema_version != 1 {
            bail!(
                "unsupported allowlist schema_version: {}",
                raw.schema_version
            );
        }
        let mut entries = Vec::with_capacity(raw.entries.len());
        let mut keys = BTreeSet::new();
        for (i, e) in raw.entries.into_iter().enumerate() {
            if e.reason.trim().chars().count() < 10 {
                bail!(
                    "allowlist entry #{i}: reason must be >= 10 characters (got {:?})",
                    e.reason
                );
            }
            if e.added_phase > 5 {
                bail!(
                    "allowlist entry #{i}: added_phase must be 0..=5 (got {})",
                    e.added_phase
                );
            }
            if e.owner.trim().is_empty() {
                bail!("allowlist entry #{i}: owner must not be empty");
            }
            let glob = Glob::new(&e.file_glob)
                .with_context(|| format!("entry #{i}: invalid file_glob {:?}", e.file_glob))?;
            let key = EntryKey {
                file_glob: e.file_glob.clone(),
                callsite: e.callsite.clone(),
                literal: e.literal.clone(),
            };
            keys.insert(key.clone());
            entries.push(Entry {
                file_matcher: glob.compile_matcher(),
                callsite: e.callsite,
                literal: e.literal,
                key,
            });
        }
        Ok(Self {
            schema_version: raw.schema_version,
            phase5_baseline_count: raw.phase5_baseline_count,
            entries,
            keys,
        })
    }

    /// Membership predicate: O(N) over entries; result is order-invariant
    /// (any matching entry yields the same boolean), so allowlist permutation
    /// does not affect outcome.
    pub fn allows(&self, file_rel: &Path, callsite: &str, literal: &str) -> bool {
        self.entries.iter().any(|e| {
            e.file_matcher.is_match(file_rel)
                && callsite_matches(&e.callsite, callsite)
                && e.literal == literal
        })
    }

    #[allow(dead_code)]
    pub fn keys(&self) -> &BTreeSet<EntryKey> {
        &self.keys
    }
}

/// Allowlist callsites are written as the trailing path segments
/// (e.g. `Text::new`) but the visitor reports the full path string
/// (e.g. `crate::ui::Text::new`). Match by suffix on `::`-segments;
/// also tolerate the leading `.` used for method-style callsites.
fn callsite_matches(entry: &str, actual: &str) -> bool {
    let entry = entry.trim_start_matches('.');
    let actual = actual.trim_start_matches('.');
    if entry == actual {
        return true;
    }
    let entry_segs: Vec<&str> = entry.split("::").collect();
    let actual_segs: Vec<&str> = actual.split("::").collect();
    if entry_segs.len() > actual_segs.len() {
        return false;
    }
    actual_segs[actual_segs.len() - entry_segs.len()..] == entry_segs[..]
}
