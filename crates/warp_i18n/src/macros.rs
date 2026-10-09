//! Runtime helpers for i18n macros.
//!
//! Compile-time `t!()` lives in `warp_i18n_macros`; this module provides the runtime
//! `tr!()` companion plus warn-deduplication for fallback hits.

use std::collections::HashSet;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::{FluentArgs, render};

static WARN_SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn warn_once(key: &str) {
    let set = WARN_SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let mut seen = set.lock();
    if seen.insert(key.to_owned()) {
        tracing::warn!(
            key,
            "warp_i18n: dynamic tr!() key not in compile-time index"
        );
    }
}

/// Runtime equivalent of `t!()`: accepts a dynamic key. The macro form
/// in [`crate::tr`] is preferred for ergonomics; this function backs it.
pub fn tr_dynamic(key: &str, args: Option<&FluentArgs>) -> String {
    if !is_known_key(key) {
        warn_once(key);
    }
    render(key, args)
}

/// Stub: with a runtime-provided dynamic key we cannot consult the compile-time phf set
/// from this crate; the proc-macro crate owns it. Callers that need strict mode should
/// guard their own keys.
fn is_known_key(_key: &str) -> bool {
    true
}

/// Dynamic-key macro. Use only when the key is computed at runtime; otherwise prefer
/// `t!("literal-key")` for compile-time validation.
///
/// ```ignore
/// let key = format!("status-{}", state);
/// let s = warp_i18n::tr!(&key);
/// let s = warp_i18n::tr!(&key, count = 3);
/// ```
#[macro_export]
macro_rules! tr {
    ($key:expr $(,)?) => {
        $crate::macros::tr_dynamic($key, None)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $(
            args.set(stringify!($name), $crate::FluentValue::from($value));
        )+
        $crate::macros::tr_dynamic($key, Some(&args))
    }};
}

/// `t!()` variant that returns `&'static str` via leak-once interning. Use only at
/// callsites that require `&'static str` (e.g. legacy widget APIs); prefer `t!()`
/// for owned-string contexts.
#[macro_export]
macro_rules! t_static {
    ($key:literal $(,)?) => {
        $crate::intern($crate::t!($key))
    };
    ($key:literal, $($rest:tt)*) => {
        $crate::intern($crate::t!($key, $($rest)*))
    };
}
