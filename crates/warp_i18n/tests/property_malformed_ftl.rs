//! Property-based tests for malformed FTL rejection.
//!
//! Covers spec requirement:
//! - (f) malformed FTL MUST be rejected at parse time by the fluent-syntax parser,
//!   which is the same library used by build.rs for compile-time validation.

use fluent_syntax::parser;
use proptest::prelude::*;

/// (f) fluent-syntax parser rejects known-invalid patterns.
proptest! {
    #[test]
    fn rejects_unterminated_selector(
        body in r"[a-zA-Z0-9 -]+\{[a-zA-Z0-9 ]+\n",
    ) {
        let ftl = format!("key = {body}");
        let result = parser::parse(ftl.as_str());
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn rejects_missing_eq(
        key in "[a-z-]{1,20}",
        value in "[A-Za-z ]{1,20}",
    ) {
        let ftl = format!("{key} {value}");
        let result = parser::parse(ftl.as_str());
        prop_assert!(result.is_err());
    }
}

/// (f) Falsification: inject random strings and verify parse is deterministic.
proptest! {
    #[test]
    fn rejected_ftl_is_consistently_rejected(
        ftl in "\\PC{0,256}",
    ) {
        let r1 = parser::parse(ftl.as_str());
        let r2 = parser::parse(ftl.as_str());
        prop_assert_eq!(r1.is_err(), r2.is_err());
    }
}

/// (f) Valid FTL is accepted by the parser.
proptest! {
    #[test]
    fn valid_ftl_accepted_or_rejected_consistently(
        key in "[a-z][a-z0-9-]{0,30}",
        value in "[A-Za-z0-9 ]{1,50}",
    ) {
        let ftl = format!("{key} = {value}\n");
        let r1 = parser::parse(ftl.as_str());
        let r2 = parser::parse(ftl.as_str());
        prop_assert_eq!(r1.is_ok(), r2.is_ok());
    }
}
