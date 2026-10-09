use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

use crate::locale::Locale;

// Static `&[(locale, filename, content)]` produced by `build.rs` via
// `include_str!`. Replaces a previous `RustEmbed` derive that silently
// shipped only the `en/` subtree in CI bundle builds — see build.rs.
include!(concat!(env!("OUT_DIR"), "/embedded_bundles.rs"));

/// Immutable per-locale resource set. Cloning is cheap (Arc).
pub struct LocaleBundle {
    locale: Locale,
    bundle: FluentBundle<Arc<FluentResource>>,
}

impl LocaleBundle {
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// Look up `key` and render with `args`. Returns `None` when the key is missing
    /// from this bundle (caller is expected to walk the fallback chain).
    pub fn render(&self, key: &str, args: Option<&FluentArgs>) -> Option<String> {
        let message = self.bundle.get_message(key)?;
        let pattern = message.value()?;
        let mut errors = vec![];
        let rendered = self
            .bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned();
        if !errors.is_empty() {
            tracing::warn!(?errors, key, locale = %self.locale, "fluent format errors");
        }
        Some(rendered)
    }

    /// Test helper: enumerate keys this bundle defines. Walks an externally provided index
    /// because `FluentBundle` does not expose iteration.
    pub fn contains(&self, key: &str) -> bool {
        self.bundle.get_message(key).is_some()
    }
}

/// Set of `LocaleBundle`s indexed by locale, plus the configured fallback chain.
pub struct Bundles {
    bundles: HashMap<Locale, Arc<LocaleBundle>>,
    fallback: Vec<Locale>,
}

impl Bundles {
    pub fn load() -> Result<Self> {
        let fallback = vec![Locale::ZhCn, Locale::En];
        let mut bundles = HashMap::new();
        for locale in [Locale::ZhCn, Locale::En] {
            bundles.insert(locale, Arc::new(load_locale(locale)?));
        }
        Ok(Self { bundles, fallback })
    }

    /// Build a `Bundles` from inline FTL sources for tests. Avoids the
    /// embedded production bundles so tests can exercise scenarios like
    /// asymmetric key sets without breaking `cargo xtask check-i18n --check-parity`.
    pub fn from_sources(en_ftl: &str, zh_cn_ftl: &str) -> Result<Self> {
        let fallback = vec![Locale::ZhCn, Locale::En];
        let mut bundles = HashMap::new();
        bundles.insert(
            Locale::ZhCn,
            Arc::new(parse_locale(Locale::ZhCn, zh_cn_ftl)?),
        );
        bundles.insert(Locale::En, Arc::new(parse_locale(Locale::En, en_ftl)?));
        Ok(Self { bundles, fallback })
    }

    /// Render `key` in `active`, walking the fallback chain on miss.
    /// Returns the key itself as a last-resort placeholder so UI never displays empty.
    pub fn render(&self, active: Locale, key: &str, args: Option<&FluentArgs>) -> String {
        let primary = self.bundles.get(&active).and_then(|b| b.render(key, args));
        if let Some(s) = primary {
            return s;
        }
        for fb in self.fallback.iter().filter(|l| **l != active) {
            if let Some(s) = self.bundles.get(fb).and_then(|b| b.render(key, args)) {
                tracing::debug!(key, %active, %fb, "fluent fallback hit");
                return s;
            }
        }
        tracing::warn!(key, %active, "fluent key missing in all bundles");
        format!("{{{key}}}")
    }

    pub fn locale_bundle(&self, locale: Locale) -> Option<Arc<LocaleBundle>> {
        self.bundles.get(&locale).cloned()
    }

    pub fn fallback_chain(&self) -> &[Locale] {
        &self.fallback
    }
}

fn load_locale(locale: Locale) -> Result<LocaleBundle> {
    let langid: LanguageIdentifier = locale.as_langid();
    let mut bundle: FluentBundle<Arc<FluentResource>> =
        FluentBundle::new_concurrent(vec![langid.clone()]);
    bundle.set_use_isolating(false);

    let want_locale = locale.as_bcp47();
    let mut loaded_any = false;
    for (entry_locale, filename, content) in EMBEDDED_BUNDLES.iter() {
        if *entry_locale != want_locale {
            continue;
        }
        let resource = FluentResource::try_new((*content).to_owned()).map_err(|(_, errs)| {
            anyhow::anyhow!("ftl parse errors in {entry_locale}/{filename}: {:?}", errs)
        })?;
        bundle.add_resource(Arc::new(resource)).map_err(|errs| {
            anyhow::anyhow!(
                "ftl add_resource errors in {entry_locale}/{filename}: {:?}",
                errs
            )
        })?;
        loaded_any = true;
    }
    if !loaded_any {
        anyhow::bail!("no .ftl files found for locale {}", locale);
    }
    Ok(LocaleBundle { locale, bundle })
}

fn parse_locale(locale: Locale, source: &str) -> Result<LocaleBundle> {
    let langid: LanguageIdentifier = locale.as_langid();
    let mut bundle: FluentBundle<Arc<FluentResource>> = FluentBundle::new_concurrent(vec![langid]);
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(source.to_owned())
        .map_err(|(_, errs)| anyhow::anyhow!("ftl parse errors: {:?}", errs))?;
    bundle
        .add_resource(Arc::new(resource))
        .map_err(|errs| anyhow::anyhow!("ftl add_resource errors: {:?}", errs))?;
    Ok(LocaleBundle { locale, bundle })
}

/// Convenience builder for argument-bearing keys.
pub fn args_from<'a, I>(pairs: I) -> FluentArgs<'a>
where
    I: IntoIterator<Item = (&'a str, FluentValue<'a>)>,
{
    let mut args = FluentArgs::new();
    for (k, v) in pairs {
        args.set(k, v);
    }
    args
}
