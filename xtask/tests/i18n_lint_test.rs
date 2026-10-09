use std::path::Path;

use xtask::i18n_lint::{Allowlist, Severity, SiteTable, scan_source};

fn scan(rel: &str, src: &str) -> Vec<xtask::i18n_lint::Violation> {
    let sites = SiteTable::default();
    let allow = Allowlist::empty();
    scan_source(Path::new(rel), src, &sites, &allow)
}

#[test]
fn flags_text_new_with_ui_literal() {
    let src = r#"
        fn build() {
            let _ = Text::new("Click me");
        }
    "#;
    let v = scan("crates/x/src/ui.rs", src);
    assert_eq!(v.len(), 1, "expected one violation, got {v:?}");
    assert_eq!(v[0].literal, "Click me");
    assert_eq!(v[0].severity, Severity::Soft);
    assert_eq!(v[0].callsite, "Text::new");
}

#[test]
fn flags_single_word_ui_literal() {
    let src = r#"
        fn build() {
            let _ = Button::new("Save");
            let _ = MenuItem::new("Preferences");
        }
    "#;
    let v = scan("app/src/menu.rs", src);
    assert_eq!(
        v.len(),
        2,
        "expected single-word UI literals reported: {v:?}"
    );
    let literals: Vec<_> = v.iter().map(|x| x.literal.as_str()).collect();
    assert!(literals.contains(&"Save"));
    assert!(literals.contains(&"Preferences"));
}

#[test]
fn skips_pure_identifier_literals() {
    // Strings without any alphabetic char (URLs without alpha, pure
    // numeric/punct config tokens) should not be flagged.
    let src = r#"
        fn build() {
            let _ = Text::new("12345");
            let _ = Text::new("--");
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert!(
        v.is_empty(),
        "purely non-alphabetic literals slipped: {v:?}"
    );
}

#[test]
fn flags_method_chain_label() {
    let src = r#"
        fn build() {
            let _ = something.label("Save changes");
        }
    "#;
    let v = scan("app/src/foo.rs", src);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].callsite, ".label");
    assert_eq!(v[0].literal, "Save changes");
}

#[test]
fn skips_tracing_macro_literal() {
    let src = r#"
        fn build() {
            tracing::info!("Loading user config");
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert!(v.is_empty(), "expected no violations, got {v:?}");
}

#[test]
fn skips_println_literal() {
    let src = r#"
        fn build() {
            println!("hello world");
            eprintln!("oops");
            panic!("bad state");
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert!(v.is_empty());
}

#[test]
fn skips_test_filename() {
    let src = r#"
        fn build() {
            let _ = Button::new("Click me");
        }
    "#;
    let v = scan("crates/x/src/foo_test.rs", src);
    assert!(v.is_empty());
    let v2 = scan("crates/x/src/foo_tests.rs", src);
    assert!(v2.is_empty());
}

#[test]
fn skips_cfg_test_module() {
    let src = r#"
        #[cfg(test)]
        mod tests {
            fn it_works() {
                let _ = Button::new("Click me");
            }
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert!(v.is_empty());
}

#[test]
fn cfg_not_test_module_is_not_suppressed() {
    let src = r#"
        #[cfg(not(test))]
        mod production {
            fn build() {
                let _ = Button::new("Click me");
            }
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert_eq!(
        v.len(),
        1,
        "cfg(not(test)) must not suppress production code: {v:?}"
    );
}

#[test]
fn skips_fmt_display_impl() {
    let src = r#"
        impl std::fmt::Display for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let label = "internal label";
                write!(f, "{label}")
            }
        }
    "#;
    let v = scan("crates/x/src/foo.rs", src);
    assert!(v.is_empty());
}

#[test]
fn skips_underscore_const() {
    let src = r#"
        const _UNUSED: &str = "Internal label not for users";
        const VISIBLE_LABEL: &str = "this is fine actually";
    "#;
    // const initializers aren't UI call sites, so no violations either way.
    let v = scan("crates/x/src/foo.rs", src);
    assert!(v.is_empty());
}

#[test]
fn allowlist_hits_suppress_violation() {
    let src = r#"
        fn build() {
            let _ = Text::new("Sample UI label");
        }
    "#;
    let toml_src = r#"
        schema_version = 1
        phase5_baseline_count = 0

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "Sample UI label"
        reason = "demo allowlist entry for unit test"
        added_phase = 0
        owner = "i18n-team"
    "#;
    let allow = Allowlist::parse(toml_src).expect("parses");
    let sites = SiteTable::default();
    let v = scan_source(Path::new("crates/x/src/foo.rs"), src, &sites, &allow);
    assert!(v.is_empty(), "allowlist did not suppress: {v:?}");
}

#[test]
fn allowlist_callsite_suffix_suppresses_qualified_call() {
    let src = r#"
        fn build() {
            let _ = crate::ui::Text::new("Sample UI label");
        }
    "#;
    let toml_src = r#"
        schema_version = 1
        phase5_baseline_count = 0

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "Sample UI label"
        reason = "demo allowlist suffix entry for unit test"
        added_phase = 0
        owner = "i18n-team"
    "#;
    let allow = Allowlist::parse(toml_src).expect("parses");
    let sites = SiteTable::default();
    let v = scan_source(Path::new("crates/x/src/foo.rs"), src, &sites, &allow);
    assert!(v.is_empty(), "suffix allowlist failed to suppress: {v:?}");
}

#[test]
fn warp_cli_glob_excluded_at_walk_level() {
    let sites = SiteTable::default();
    let repo = Path::new("/repo");
    assert!(sites.is_excluded(Path::new("/repo/crates/warp_cli/src/main.rs"), repo));
    assert!(sites.is_excluded(Path::new("/repo/crates/warp_cli_remote/src/lib.rs"), repo));
    assert!(!sites.is_excluded(Path::new("/repo/crates/warp_terminal/src/lib.rs"), repo));
}

#[test]
fn flags_explicit_remote_string_token_as_hard() {
    // Strong signal — `RemoteString` appears literally inside the macro args
    // (UFCS, turbofish, or explicit cast). The visitor cannot resolve types,
    // but this token-level mention is unambiguous misuse.
    let src = r#"
        fn render() {
            let _ = t!(<RemoteString as AsRef<str>>::as_ref(&value));
        }
    "#;
    let v = scan("app/src/foo.rs", src);
    let hard: Vec<_> = v.iter().filter(|x| x.severity == Severity::Hard).collect();
    assert!(
        !hard.is_empty(),
        "explicit RemoteString must hard-fail: {v:?}"
    );
    assert!(hard[0].message.contains("RemoteString"));
}

#[test]
fn flags_as_str_call_as_hard() {
    let src = r#"
        fn render() {
            let _ = t!(notification.title.as_str());
        }
    "#;
    let v = scan("app/src/foo.rs", src);
    let hard: Vec<_> = v.iter().filter(|x| x.severity == Severity::Hard).collect();
    assert!(!hard.is_empty(), "expected hard violation: {v:?}");
    assert!(hard[0].message.contains("RemoteString"));
}

#[test]
fn flags_remote_field_access_in_tr_key_position_as_hard() {
    let src = r#"
        fn render() {
            let _ = tr!(Locale::En, notification.title.as_str());
        }
    "#;
    let v = scan("app/src/foo.rs", src);
    let hard: Vec<_> = v.iter().filter(|x| x.severity == Severity::Hard).collect();
    assert!(!hard.is_empty(), "tr! second arg not hard-flagged: {v:?}");
}

#[test]
fn allowlist_match_is_order_invariant() {
    let toml_a = r#"
        schema_version = 1
        phase5_baseline_count = 0

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "alpha"
        reason = "first ordering for commutativity test"
        added_phase = 0
        owner = "i18n-team"

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "beta"
        reason = "second ordering for commutativity test"
        added_phase = 0
        owner = "i18n-team"
    "#;
    let toml_b = r#"
        schema_version = 1
        phase5_baseline_count = 0

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "beta"
        reason = "second ordering for commutativity test"
        added_phase = 0
        owner = "i18n-team"

        [[entries]]
        file_glob = "crates/x/**/*.rs"
        callsite = "Text::new"
        literal = "alpha"
        reason = "first ordering for commutativity test"
        added_phase = 0
        owner = "i18n-team"
    "#;
    let a = Allowlist::parse(toml_a).unwrap();
    let b = Allowlist::parse(toml_b).unwrap();
    assert_eq!(a.keys(), b.keys());
}

#[test]
fn allowlist_rejects_short_reason() {
    let toml_src = r#"
        schema_version = 1
        phase5_baseline_count = 0

        [[entries]]
        file_glob = "**/*.rs"
        callsite = "Text::new"
        literal = "x"
        reason = "short"
        added_phase = 0
        owner = "i18n-team"
    "#;
    assert!(Allowlist::parse(toml_src).is_err());
}
