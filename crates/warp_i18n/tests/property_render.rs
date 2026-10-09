//! Property-based tests for render purity and side-effect freedom.
//!
//! Covers spec requirements:
//! - (a) render is a pure function: fixed (locale, key, args) -> bitwise equal String
//! - (c) render has no side effects: global state, bundle contents, args must be unmodified

use std::sync::{Mutex, OnceLock};

use proptest::prelude::*;
use warp_i18n::loader::Bundles;
use warp_i18n::{FluentValue, Locale, args_from, render, set_locale};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// (a) render with known production keys is deterministic.
proptest! {
    #[test]
    fn render_known_keys_deterministic(
        key in prop::sample::select(&[
            "ui-button-ok", "ui-button-cancel", "ui-button-save",
            "ui-button-discard", "ui-button-close", "ui-button-confirm",
            "ui-button-retry", "ui-button-default",
        ]),
        locale in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
    ) {
        let _g = lock().lock().unwrap();
        let _ = warp_i18n::init(locale);
        set_locale(locale);
        let first = render(key, None);
        for _ in 0..500 {
            prop_assert_eq!(&render(key, None), &first);
        }
    }
}

/// (a) Falsification: with named args, render is deterministic.
proptest! {
    #[test]
    fn render_with_args_deterministic(
        n in 0u32..1000u32,
        locale in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
    ) {
        let _g = lock().lock().unwrap();
        let _ = warp_i18n::init(locale);
        set_locale(locale);
        let args = args_from([("n", FluentValue::from(n))]);
        let first = render("tabs-close-confirm", Some(&args));
        for _ in 0..500 {
            prop_assert_eq!(&render("tabs-close-confirm", Some(&args)), &first);
        }
    }
}

/// (a) render with synthetic bundles is deterministic.
proptest! {
    #[test]
    fn render_deterministic_with_synthetic(
        key in r"[a-z][a-z0-9-]{0,20}",
        en_val in r"[A-Za-z0-9 ]{1,30}",
        zh_val in r"[A-Za-z0-9 ]{1,30}",
        locale in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
    ) {
        let en_ftl = format!("{key} = {en_val}\n");
        let zh_ftl = format!("{key} = {zh_val}\n");
        let Ok(bundles) = Bundles::from_sources(&en_ftl, &zh_ftl) else {
            return Ok(());
        };

        let first = bundles.render(locale, &key, None);
        for _ in 0..200 {
            prop_assert_eq!(&bundles.render(locale, &key, None), &first);
        }
    }
}

/// (c) render does not mutate global state.
proptest! {
    #[test]
    fn render_has_no_side_effects(
        key in prop::sample::select(&[
            "ui-button-ok", "ui-button-cancel", "ui-button-save",
        ]),
        locale in prop_oneof![Just(Locale::ZhCn), Just(Locale::En)],
    ) {
        let _g = lock().lock().unwrap();
        let _ = warp_i18n::init(locale);
        set_locale(locale);
        let locale_before = warp_i18n::current_locale();
        let val_before = render(key, None);

        for _ in 0..300 {
            let _ = render(key, None);
        }

        prop_assert_eq!(warp_i18n::current_locale(), locale_before);
        prop_assert_eq!(render(key, None), val_before);
    }
}
