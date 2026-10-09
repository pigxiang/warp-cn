//! Property-based tests for init idempotence and set_locale idempotence.
//!
//! Covers spec requirement:
//! - (b) init() and set_locale are idempotent

use std::sync::{Mutex, OnceLock};

use proptest::prelude::*;
use warp_i18n::{Locale, current_locale, global, init, set_locale};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// (b) init is idempotent: calling init() N times returns the same &'static I18n.
proptest! {
    #[test]
    fn init_is_idempotent(
        calls in 1u32..20u32,
    ) {
        let _g = lock().lock().unwrap();
        let _ = init(Locale::ZhCn);
        let ptr = global() as *const _;
        for _ in 0..calls {
            let _ = init(Locale::En);
            let new_ptr = global() as *const _;
            prop_assert_eq!(ptr, new_ptr);
        }
    }
}

/// (b) set_locale to the same value is effectively idempotent (no-op).
proptest! {
    #[test]
    fn set_locale_repeated_is_idempotent(
        locale in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
        calls in 1u32..10u32,
    ) {
        let _g = lock().lock().unwrap();
        let _ = init(locale);
        set_locale(locale);
        let before = current_locale();
        for _ in 0..calls {
            set_locale(locale);
            prop_assert_eq!(current_locale(), before);
        }
    }
}

/// (b) Falsification: arbitrary init sequences produce a valid global instance.
proptest! {
    #[test]
    fn init_always_produces_valid_global(
        initial in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
        secondary in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
    ) {
        let _g = lock().lock().unwrap();
        let _ = init(initial);
        let i18n = global();
        let loc = i18n.current();
        prop_assert!(matches!(loc, Locale::ZhCn | Locale::En));

        let _ = init(secondary);
        let loc2 = current_locale();
        prop_assert_eq!(loc2, loc);
    }
}
