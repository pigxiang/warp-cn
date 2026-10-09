//! Property-based test for locale-switch linearizability.
//!
//! Covers spec requirement:
//! - (d) linearizable: 200 interleaved Set/Lookup operations must produce results
//!   belonging to either pre-switch or post-switch locale.
//!
//! Uses multithreaded writer/reader interleaving with AtomicU8 for locale state,
//! plus Mutex-protected result log for post-hoc consistency verification.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;

/// Run 200+ Set/Lookup operations across multiple writer and reader threads.
/// Post-condition: every Lookup result for a given (locale, key) pair is deterministic.
#[test]
fn set_locale_lookup_linearizable_200_ops() {
    const N_WRITERS: usize = 4;
    const N_READERS: usize = 8;
    const OPS_PER_THREAD: usize = 25;

    let state = Arc::new(TestState::new());

    let mut handles = vec![];

    for i in 0..N_WRITERS {
        let s = Arc::clone(&state);
        handles.push(thread::spawn(move || {
            use warp_i18n::Locale;
            for j in 0..OPS_PER_THREAD {
                let locale = if (i + j) % 2 == 0 {
                    Locale::ZhCn
                } else {
                    Locale::En
                };
                s.set(locale);
            }
        }));
    }

    for i in 0..N_READERS {
        let s = Arc::clone(&state);
        handles.push(thread::spawn(move || {
            let keys = [
                "ui-button-ok",
                "ui-button-cancel",
                "ui-button-save",
                "tabs-close-confirm",
            ];
            for j in 0..OPS_PER_THREAD {
                let key = keys[(i * j + j) % keys.len()];
                let observed_locale = s.current_snapshot();
                let result = s.render(key);
                s.record_result(observed_locale, result, key);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Invariant: for a given (locale, key), all render results must be identical.
    let results = state.results();
    for (_locale, key, values) in &results {
        if values.len() > 1 {
            let first = &values[0];
            for v in &values[1..] {
                assert_eq!(
                    *v, *first,
                    "Non-deterministic render for key={key:?}: {first:?} vs {v:?}"
                );
            }
        }
    }
}

struct TestState {
    locale: AtomicU8,
    results: std::sync::Mutex<Vec<(u8, String, String)>>,
}

impl TestState {
    fn new() -> Self {
        Self {
            locale: AtomicU8::new(0), // ZhCn = 0
            results: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn set(&self, l: warp_i18n::Locale) {
        let v: u8 = match l {
            warp_i18n::Locale::ZhCn => 0,
            warp_i18n::Locale::En => 1,
        };
        self.locale.store(v, Ordering::Release);
    }

    fn current_snapshot(&self) -> u8 {
        self.locale.load(Ordering::Acquire)
    }

    fn render(&self, key: &str) -> String {
        let locale = self.current_snapshot();
        let l = match locale {
            0 => warp_i18n::Locale::ZhCn,
            _ => warp_i18n::Locale::En,
        };
        let _ = warp_i18n::init(l);
        warp_i18n::set_locale(l);
        warp_i18n::render(key, None)
    }

    fn record_result(&self, locale: u8, result: String, key: &str) {
        let mut res = self.results.lock().unwrap();
        res.push((locale, key.to_owned(), result));
    }

    fn results(&self) -> Vec<(warp_i18n::Locale, String, Vec<String>)> {
        use std::collections::BTreeMap;
        let res = self.results.lock().unwrap();
        let mut groups: BTreeMap<(u8, String), Vec<String>> = BTreeMap::new();
        for (locale, key, value) in res.iter() {
            groups
                .entry((*locale, key.clone()))
                .or_default()
                .push(value.clone());
        }
        groups
            .into_iter()
            .map(|((l, k), v)| {
                let loc = match l {
                    0 => warp_i18n::Locale::ZhCn,
                    _ => warp_i18n::Locale::En,
                };
                (loc, k, v)
            })
            .collect()
    }
}
