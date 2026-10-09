use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

/// UI construction sites whose string-literal arguments must flow through `t!()`.
///
/// Two flavors:
/// - **Calls** (`Text::new`, `Menu::new`, ...): the *first* string-literal positional
///   argument is the user-visible label.
/// - **Methods** (`.label`, `.tooltip`, `.title`, ...): chained builder methods whose
///   *first* string-literal argument is the user-visible value.
pub struct SiteTable {
    pub call_sites: Vec<&'static str>,
    pub method_sites: Vec<&'static str>,
    excludes: GlobSet,
}

impl Default for SiteTable {
    fn default() -> Self {
        Self {
            call_sites: vec![
                "Text::new",
                "Button::new",
                "Menu::new",
                "MenuItem::new",
                "Dialog::message",
                "Toast::new",
                "Banner::new",
                "Notification::new",
            ],
            method_sites: vec!["label", "tooltip", "title", "placeholder", "description"],
            excludes: build_excludes(&[
                "crates/warp_cli/**",
                "crates/warp_cli_*/**",
                "**/build.rs",
                "**/target/**",
                "**/examples/**",
            ]),
        }
    }
}

impl SiteTable {
    /// Whether the file should be skipped entirely (CLI crates, generated code, etc.).
    pub fn is_excluded(&self, path: &Path, repo_root: &Path) -> bool {
        let rel = path.strip_prefix(repo_root).unwrap_or(path);
        self.excludes.is_match(rel)
    }

    pub fn is_call_site(&self, path_repr: &str) -> bool {
        self.call_sites.iter().any(|s| match_call(s, path_repr))
    }

    pub fn is_method_site(&self, method: &str) -> bool {
        self.method_sites.iter().any(|s| *s == method)
    }
}

fn build_excludes(patterns: &[&str]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        if let Ok(glob) = Glob::new(p) {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// Match a call-site path representation. We accept either the exact pattern
/// (e.g. `Text::new`) or a fully-qualified suffix match
/// (e.g. `crate::ui::Text::new` matches `Text::new`).
fn match_call(pattern: &str, path_repr: &str) -> bool {
    if path_repr == pattern {
        return true;
    }
    path_repr
        .rsplit_once("::")
        .map(|(_, tail)| {
            // For type-associated calls like `Text::new`, the rendered path
            // ends with the function name. We match if the suffix `Type::fn`
            // (last two segments) matches the pattern.
            let segments: Vec<&str> = path_repr.split("::").collect();
            if segments.len() >= 2 {
                let last2 = format!(
                    "{}::{}",
                    segments[segments.len() - 2],
                    segments[segments.len() - 1]
                );
                last2 == pattern || tail == pattern
            } else {
                tail == pattern
            }
        })
        .unwrap_or(false)
}
