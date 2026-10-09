//! Warp i18n runtime.
//!
//! Architecture (D18):
//! - `init()` constructs the global [`I18n`] once. Subsequent calls are no-ops.
//! - Reads (`render`, [`current_locale`]) are lock-free: `ArcSwap::load` + immutable bundle snapshot.
//! - Writes (`set_locale`) publish via `ArcSwap::store` *before* notifying watchers, so
//!   any watcher woken by the watch channel observing the new value will also observe
//!   the corresponding locale through the snapshot pointer.

pub mod intern;
pub mod loader;
pub mod locale;
pub mod macros;
pub mod notification;

use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
pub use fluent_bundle::{FluentArgs, FluentValue};
use once_cell::sync::OnceCell;
use tokio::sync::watch;
pub use warp_i18n_macros::t;

pub use crate::intern::intern;
pub use crate::loader::{Bundles, LocaleBundle, args_from};
pub use crate::locale::{LanguagePreference, Locale, detect_system_locale, resolve_locale};

static I18N: OnceCell<I18n> = OnceCell::new();

pub struct I18n {
    bundles: Arc<Bundles>,
    active: ArcSwap<Locale>,
    watch_tx: watch::Sender<Locale>,
}

impl I18n {
    pub fn current(&self) -> Locale {
        **self.active.load()
    }

    /// Render `key` in the currently active locale; falls back per [`Bundles`] policy.
    pub fn render(&self, key: &str, args: Option<&FluentArgs>) -> String {
        self.bundles.render(self.current(), key, args)
    }

    /// Force the active locale. Order is critical: publish snapshot first, then
    /// wake watchers. Watchers reading current locale via [`current`] will see the
    /// new value (D18).
    pub fn set_locale(&self, locale: Locale) {
        self.active.store(Arc::new(locale));
        // `send` returns Err only when no receivers exist, which is fine.
        let _ = self.watch_tx.send(locale);
    }

    pub fn subscribe(&self) -> watch::Receiver<Locale> {
        self.watch_tx.subscribe()
    }

    pub fn bundles(&self) -> &Bundles {
        &self.bundles
    }
}

/// Initialise the global i18n with `initial`. Idempotent: subsequent calls return the
/// existing instance and do *not* swap the locale.
pub fn init(initial: Locale) -> Result<&'static I18n> {
    if let Some(existing) = I18N.get() {
        return Ok(existing);
    }
    let bundles = Arc::new(Bundles::load()?);
    let (watch_tx, _watch_rx) = watch::channel(initial);
    let i18n = I18n {
        bundles,
        active: ArcSwap::new(Arc::new(initial)),
        watch_tx,
    };
    // Race: another thread may have won. `set` only fails if already set, in which case
    // the existing instance is what callers want.
    let _ = I18N.set(i18n);
    Ok(I18N.get().expect("just-initialised"))
}

/// Access the global instance. Panics if [`init`] has not been called — callers in app
/// startup paths must call [`init`] first; tests may use [`init_for_test`].
pub fn global() -> &'static I18n {
    I18N.get()
        .expect("warp_i18n::init() must be called before render / current_locale")
}

/// Try to access the global instance without panicking. Returns `None` pre-init.
pub fn try_global() -> Option<&'static I18n> {
    I18N.get()
}

pub fn current_locale() -> Locale {
    try_global().map(I18n::current).unwrap_or(Locale::DEFAULT)
}

pub fn set_locale(locale: Locale) {
    if let Some(i18n) = try_global() {
        i18n.set_locale(locale);
    }
}

/// Render `key` against the global instance. Falls back to the bare key wrapped in
/// `{...}` when `init` has not been called (test paths and early startup).
pub fn render(key: &str, args: Option<&FluentArgs>) -> String {
    match try_global() {
        Some(i18n) => i18n.render(key, args),
        None => format!("{{{key}}}"),
    }
}

#[cfg(test)]
mod lib_tests;
