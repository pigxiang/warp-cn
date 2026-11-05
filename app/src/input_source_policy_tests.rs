use super::*;

#[test]
fn starts_with_exempts_only_prefixed_titles() {
    assert!(title_is_exempt(
        "OC-session-1",
        PaneTitleMatchMode::StartsWith,
        "OC"
    ));
    assert!(!title_is_exempt(
        "My OC pane",
        PaneTitleMatchMode::StartsWith,
        "OC"
    ));
    assert!(!title_is_exempt(
        "oc-pane",
        PaneTitleMatchMode::StartsWith,
        "OC"
    ));
}

#[test]
fn not_starts_with_exempts_unprefixed_titles() {
    assert!(!title_is_exempt(
        "OC-session-1",
        PaneTitleMatchMode::NotStartsWith,
        "OC"
    ));
    assert!(title_is_exempt(
        "My OC pane",
        PaneTitleMatchMode::NotStartsWith,
        "OC"
    ));
}

#[test]
fn contains_exempts_anywhere_matches() {
    assert!(title_is_exempt(
        "OC-session-1",
        PaneTitleMatchMode::Contains,
        "OC"
    ));
    assert!(title_is_exempt(
        "My OC pane",
        PaneTitleMatchMode::Contains,
        "OC"
    ));
    assert!(!title_is_exempt(
        "plain terminal",
        PaneTitleMatchMode::Contains,
        "OC"
    ));
}

#[test]
fn not_contains_exempts_titles_without_pattern() {
    assert!(!title_is_exempt(
        "My OC pane",
        PaneTitleMatchMode::NotContains,
        "OC"
    ));
    assert!(title_is_exempt(
        "plain terminal",
        PaneTitleMatchMode::NotContains,
        "OC"
    ));
}

#[test]
fn empty_title_is_exempt_only_for_negative_modes() {
    assert!(!title_is_exempt("", PaneTitleMatchMode::StartsWith, "OC"));
    assert!(title_is_exempt("", PaneTitleMatchMode::NotStartsWith, "OC"));
    assert!(!title_is_exempt("", PaneTitleMatchMode::Contains, "OC"));
    assert!(title_is_exempt("", PaneTitleMatchMode::NotContains, "OC"));
}

#[test]
fn empty_pattern_exempts_all_titles_except_negative_contains() {
    assert!(title_is_exempt(
        "anything",
        PaneTitleMatchMode::StartsWith,
        ""
    ));
    assert!(title_is_exempt(
        "anything",
        PaneTitleMatchMode::Contains,
        ""
    ));
    assert!(!title_is_exempt(
        "anything",
        PaneTitleMatchMode::NotStartsWith,
        ""
    ));
    assert!(!title_is_exempt(
        "anything",
        PaneTitleMatchMode::NotContains,
        ""
    ));
}
