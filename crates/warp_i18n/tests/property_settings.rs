//! Property-based tests for settings serialization roundtrip.
//!
//! Covers spec requirement:
//! - (g) settings serialization roundtrip

use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use warp_i18n::locale::LanguagePreference;

/// (g) LanguagePreference serialization roundtrip: deserialize(serialize(x)) == x.
proptest! {
    #[test]
    fn language_preference_serde_roundtrip(
        pref in prop_oneof![
            Just(LanguagePreference::Zh),
            Just(LanguagePreference::En),
            Just(LanguagePreference::System),
        ],
    ) {
        let json = serde_json::to_string(&pref).expect("serialize LanguagePreference");
        let back: LanguagePreference = serde_json::from_str(&json).expect("deserialize LanguagePreference");
        assert_eq!(pref, back, "serde roundtrip failed for {pref:?} json={json}");
    }
}

/// (g) Falsification: arbitrary wrapper struct with injected noise fields.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FakeSettings {
    #[serde(default)]
    language: Option<LanguagePreference>,
    #[serde(default)]
    other_field: Option<String>,
    #[serde(default)]
    noise: Option<i32>,
}

proptest! {
    #[test]
    fn settings_language_field_roundtrip(
        lang in prop_oneof![
            Just(None),
            Just(Some(LanguagePreference::Zh)),
            Just(Some(LanguagePreference::En)),
            Just(Some(LanguagePreference::System)),
        ],
        other in prop::option::of("[a-zA-Z0-9_ ]{0,20}"),
        noise in prop::option::of(-1000i32..1000i32),
    ) {
        let settings = FakeSettings { language: lang, other_field: other.clone(), noise };
        let json = serde_json::to_string(&settings).expect("serialize");
        let back: FakeSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(settings.language, back.language, "language field must roundtrip");
        assert_eq!(other, back.other_field, "unrelated field must roundtrip");
        assert_eq!(noise, back.noise, "unrelated field must roundtrip");
    }
}

/// (g) Unknown string values in LanguagePreference produce a serde error.
/// Serde enum deserialization rejects unknown variants by default.
#[test]
fn unknown_language_string_rejected_by_serde() {
    let s = r#"{"language": "ja-JP"}"#;
    let result: Result<FakeSettings, _> = serde_json::from_str(s);
    assert!(
        result.is_err(),
        "unknown language string must be rejected by serde"
    );
}
