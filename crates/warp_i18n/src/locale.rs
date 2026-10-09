use std::fmt;

use serde::{Deserialize, Serialize};
use unic_langid::{LanguageIdentifier, langid};

/// Supported UI locales.
///
/// Forks add new variants here and update the loader fallback chain in `loader.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Locale {
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en")]
    En,
}

impl Locale {
    /// Default locale for this fork: simplified Chinese.
    pub const DEFAULT: Self = Self::ZhCn;

    pub fn as_langid(&self) -> LanguageIdentifier {
        match self {
            Self::ZhCn => langid!("zh-CN"),
            Self::En => langid!("en"),
        }
    }

    pub fn as_bcp47(&self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    /// Parse a BCP-47 tag into one of the supported locales.
    ///
    /// Normalisation rules: leading whitespace and trailing region/script subtags are tolerated;
    /// any `zh*` tag maps to `ZhCn`; `en*` maps to `En`. Unknown tags return `None`.
    pub fn parse_bcp47(tag: &str) -> Option<Self> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return None;
        }
        let primary = trimmed.split(['-', '_']).next().unwrap_or(trimmed);
        match primary.to_ascii_lowercase().as_str() {
            "zh" => Some(Self::ZhCn),
            "en" => Some(Self::En),
            _ => None,
        }
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_bcp47())
    }
}

/// User-facing language preference persisted in settings (D13).
///
/// Disk representation:
/// - `"zh-CN"` / `"en"` → `Explicit(Locale::*)`
/// - `"system"`         → `FollowSystem`
/// - missing field      → `None` at parse boundary, mapped to `LanguagePreference::default()`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguagePreference {
    #[serde(rename = "zh-CN")]
    Zh,
    #[serde(rename = "en")]
    En,
    System,
}

impl Default for LanguagePreference {
    /// Fork default: zh-CN, NOT system locale (D13).
    fn default() -> Self {
        Self::Zh
    }
}

/// Resolve the active locale from a user preference plus the host system tag.
///
/// `system_tag` should come from `sys-locale::get_locale()`; pass `None` to opt out
/// of system detection entirely (test paths, headless processes).
pub fn resolve_locale(pref: Option<LanguagePreference>, system_tag: Option<&str>) -> Locale {
    match pref.unwrap_or_default() {
        LanguagePreference::Zh => Locale::ZhCn,
        LanguagePreference::En => Locale::En,
        LanguagePreference::System => system_tag
            .and_then(Locale::parse_bcp47)
            .unwrap_or(Locale::En),
    }
}

/// Detect the host system locale via `sys-locale`. Cached lookup is safe for repeat callers.
pub fn detect_system_locale() -> Option<String> {
    sys_locale::get_locale()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bcp47_handles_chinese_variants() {
        for tag in ["zh-CN", "zh-Hans-CN", "zh", "zh_TW", "  zh-Hant  "] {
            assert_eq!(Locale::parse_bcp47(tag), Some(Locale::ZhCn), "tag={tag}");
        }
    }

    #[test]
    fn parse_bcp47_handles_english_variants() {
        for tag in ["en", "en-US", "en_GB"] {
            assert_eq!(Locale::parse_bcp47(tag), Some(Locale::En), "tag={tag}");
        }
    }

    #[test]
    fn parse_bcp47_rejects_unknown() {
        assert_eq!(Locale::parse_bcp47(""), None);
        assert_eq!(Locale::parse_bcp47("ja"), None);
        assert_eq!(Locale::parse_bcp47("fr-FR"), None);
    }

    #[test]
    fn resolve_explicit_overrides_system() {
        assert_eq!(
            resolve_locale(Some(LanguagePreference::En), Some("zh-CN")),
            Locale::En
        );
        assert_eq!(
            resolve_locale(Some(LanguagePreference::Zh), Some("en-US")),
            Locale::ZhCn
        );
    }

    #[test]
    fn resolve_system_falls_back_to_en_when_unknown() {
        assert_eq!(
            resolve_locale(Some(LanguagePreference::System), Some("ja-JP")),
            Locale::En
        );
        assert_eq!(
            resolve_locale(Some(LanguagePreference::System), None),
            Locale::En
        );
    }

    #[test]
    fn missing_preference_defaults_to_zh() {
        assert_eq!(resolve_locale(None, Some("en-US")), Locale::ZhCn);
        assert_eq!(resolve_locale(None, None), Locale::ZhCn);
    }
}
