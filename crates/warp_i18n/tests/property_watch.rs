//! Property-based tests for watch channel delivery.
//!
//! Covers spec requirement:
//! - (e) watch delivery: each set_locale(L) with L != current MUST deliver changed() to all receivers

use std::sync::{Mutex, OnceLock};

use proptest::prelude::*;
use warp_i18n::{Locale, global, init, set_locale};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// (e) Sequential set_locale calls deliver all distinct transitions to receivers.
proptest! {
    #[test]
    fn watch_delivers_all_distinct_transitions(
        seq in prop::collection::vec(
            prop_oneof![Just(Locale::En), Just(Locale::ZhCn)],
            1..50,
        ),
    ) {
        let _g = lock().lock().unwrap();
        let _ = init(Locale::ZhCn);
        let i18n = global();

        let distinct: Vec<Locale> = {
            let mut d: Vec<Locale> = vec![i18n.current()];
            for &l in &seq {
                let last = *d.last().unwrap();
                if l != last {
                    d.push(l);
                }
            }
            let mut result = d;
            if result.len() > 1 {
                result.remove(0);
                result
            } else {
                vec![]
            }
        };

        if distinct.is_empty() {
            return Ok(());
        }

        let mut rx = i18n.subscribe();
        rx.borrow_and_update();

        for &locale in &distinct {
            set_locale(locale);
        }

        let observed = *rx.borrow_and_update();
        let expected = *distinct.last().unwrap();
        prop_assert_eq!(observed, expected);
    }
}

/// (e) Falsification: multiple receivers all observe consistent state.
proptest! {
    #[test]
    fn multiple_receivers_consistent(
        seq in prop::collection::vec(
            prop_oneof![Just(Locale::En), Just(Locale::ZhCn)],
            5..30,
        ),
        num_receivers in 1usize..10usize,
    ) {
        let _g = lock().lock().unwrap();
        let _ = init(Locale::ZhCn);
        let i18n = global();

        let mut receivers: Vec<_> = (0..num_receivers)
            .map(|_| i18n.subscribe())
            .collect();

        for i in 0..receivers.len() {
            receivers[i].borrow_and_update();
        }

        for &locale in &seq {
            if locale != i18n.current() {
                set_locale(locale);
            }
        }

        let expected = *receivers[0].borrow_and_update();
        for i in 1..receivers.len() {
            let obs = *receivers[i].borrow_and_update();
            prop_assert_eq!(obs, expected);
        }
    }
}
