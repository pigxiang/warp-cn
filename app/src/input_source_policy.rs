//! macOS-only policy: manage the keyboard input source on terminal pane activation.
//!
//! - Pane title matches the exemption rule: keep/restore that pane's remembered
//!   input source (snapshot taken when the pane loses focus).
//! - Pane title doesn't match: force the English keyboard layout; no-op when the
//!   current source is already English.

use std::collections::HashMap;

use settings::macros::implement_setting_for_enum;
use settings::{Setting, SupportedPlatforms, SyncToCloud};
use warpui::{AppContext, Entity, SingletonEntity, ViewHandle, WindowId};

use crate::pane_group::PaneGroup;
use crate::pane_group::pane::{PaneConfiguration, PaneId};
use crate::terminal::TerminalView;
use crate::terminal::general_settings::{GeneralSettings, GeneralSettingsChangedEvent};

/// Built-in English keyboard layout selected whenever the exemption rule doesn't
/// match. Not user-configurable on purpose: "English" must not be an IME that
/// could land in a composing state.
pub const ENGLISH_INPUT_SOURCE_ID: &str = "com.apple.keylayout.ABC";

/// Bounds the per-pane memory so long-lived sessions with churn don't grow it
/// without limit.
const MAX_REMEMBERED_PANES: usize = 512;

#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "How a pane title is matched against the input-method exemption pattern.",
    rename_all = "snake_case"
)]
pub enum PaneTitleMatchMode {
    #[default]
    StartsWith,
    NotStartsWith,
    Contains,
    NotContains,
}

implement_setting_for_enum!(
    PaneTitleMatchMode,
    GeneralSettings,
    SupportedPlatforms::MAC,
    SyncToCloud::Never,
    surface: settings::SettingSurfaces::GUI,
    private: false,
    toml_path: "general.pane_title_match_mode",
    description: "How a pane title is matched against the input-method exemption pattern.",
);

/// Whether the pane title matches the configured exemption rule. Matching is
/// case-sensitive.
#[must_use]
pub fn title_is_exempt(title: &str, mode: PaneTitleMatchMode, pattern: &str) -> bool {
    match mode {
        PaneTitleMatchMode::StartsWith => title.starts_with(pattern),
        PaneTitleMatchMode::NotStartsWith => !title.starts_with(pattern),
        PaneTitleMatchMode::Contains => title.contains(pattern),
        PaneTitleMatchMode::NotContains => !title.contains(pattern),
    }
}

/// Per-pane input source memory: the source each pane was using when it last
/// lost focus, plus the last focused pane per window so we know which pane to
/// snapshot when focus moves.
#[derive(Default)]
pub struct InputSourceMemoryModel {
    remembered: HashMap<PaneId, String>,
    last_focused: HashMap<WindowId, PaneId>,
}

impl Entity for InputSourceMemoryModel {
    type Event = ();
}

impl SingletonEntity for InputSourceMemoryModel {}

impl InputSourceMemoryModel {
    pub fn new() -> Self {
        Self::default()
    }

    fn remember(&mut self, pane_id: PaneId, source_id: &str) {
        if self.remembered.len() >= MAX_REMEMBERED_PANES {
            self.remembered.clear();
        }
        self.remembered.insert(pane_id, source_id.to_owned());
    }
}

fn policy_enabled(ctx: &AppContext) -> bool {
    *GeneralSettings::as_ref(ctx).switch_input_method_on_pane_activation
}

fn current_input_source_id(ctx: &AppContext) -> Option<String> {
    ctx.current_input_source_id()
}

fn select_source_if_needed(ctx: &AppContext, target: &str) {
    if current_input_source_id(ctx).as_deref() == Some(target) {
        return;
    }
    if !ctx.select_input_source(target) {
        log::warn!("Failed to switch input source to {target}");
    }
}

fn remember_source(ctx: &mut AppContext, pane_id: PaneId, source_id: &str) {
    InputSourceMemoryModel::handle(ctx).update(ctx, |model, _| {
        model.remember(pane_id, source_id);
    });
}

/// Whether any title source for the pane matches the exemption rule. The
/// displayed pane title can be overridden (CLI-agent event text, warpified
/// SSH conversations), so the custom rename and the raw OSC title are checked
/// as well. Empty sources never count as a match, keeping negative patterns
/// from exempting everything.
fn exemption_matches(
    view: &TerminalView,
    pane_config: &PaneConfiguration,
    mode: PaneTitleMatchMode,
    pattern: &str,
) -> bool {
    let sources = [
        Some(pane_config.title()),
        pane_config.custom_vertical_tabs_title(),
    ];
    if sources
        .into_iter()
        .flatten()
        .any(|title| !title.is_empty() && title_is_exempt(title, mode, pattern))
    {
        return true;
    }
    let raw_osc = view.terminal_title_from_shell();
    !raw_osc.is_empty() && title_is_exempt(&raw_osc, mode, pattern)
}

/// Applies the rule for `pane_id`'s title: restore its remembered source when
/// exempt, otherwise force English.
fn apply_for_pane(pane_group: &ViewHandle<PaneGroup>, pane_id: PaneId, ctx: &AppContext) {
    let pane_group = pane_group.as_ref(ctx);
    let Some(terminal_view) = pane_group.terminal_view_from_pane_id(pane_id, ctx) else {
        return;
    };
    let view = terminal_view.as_ref(ctx);
    let pane_config = view.pane_configuration().as_ref(ctx);

    let settings = GeneralSettings::as_ref(ctx);
    let mode = settings.pane_title_match_mode;
    let pattern = settings.pane_title_match_pattern.value().clone();

    if exemption_matches(view, pane_config, mode, &pattern) {
        let remembered = InputSourceMemoryModel::handle(ctx)
            .as_ref(ctx)
            .remembered
            .get(&pane_id)
            .cloned();
        if let Some(source_id) = remembered {
            select_source_if_needed(ctx, &source_id);
        }
    } else {
        select_source_if_needed(ctx, ENGLISH_INPUT_SOURCE_ID);
    }
}

/// Called when a pane gains focus within a window. Snapshots the outgoing
/// pane's current input source (still reflects the old pane's state because we
/// run before any forced switch), then applies the rule for the new pane.
pub fn on_pane_focused(
    pane_group: &ViewHandle<PaneGroup>,
    window_id: WindowId,
    ctx: &mut AppContext,
) {
    if !policy_enabled(ctx) {
        return;
    }

    let focused = pane_group.as_ref(ctx).focused_pane_id(ctx);
    let previous = InputSourceMemoryModel::handle(ctx)
        .as_ref(ctx)
        .last_focused
        .get(&window_id)
        .copied();
    if let Some(previous) = previous.filter(|p| *p != focused)
        && let Some(source_id) = current_input_source_id(ctx)
    {
        remember_source(ctx, previous, &source_id);
    }
    InputSourceMemoryModel::handle(ctx).update(ctx, |model, _| {
        model.last_focused.insert(window_id, focused);
    });

    apply_for_pane(pane_group, focused, ctx);
}

/// Called when this window becomes the active window. No outgoing-pane
/// snapshot: while Warp was inactive the system input source may have changed
/// for another app's sake.
pub fn on_window_activated(pane_group: &ViewHandle<PaneGroup>, ctx: &AppContext) {
    if !policy_enabled(ctx) {
        return;
    }
    let focused = pane_group.as_ref(ctx).focused_pane_id(ctx);
    apply_for_pane(pane_group, focused, ctx);
}

/// Called when this window is losing active-window status: snapshot the
/// focused pane's input source while it still reflects that pane's state.
pub fn on_window_deactivated(pane_group: &ViewHandle<PaneGroup>, ctx: &mut AppContext) {
    if !policy_enabled(ctx) {
        return;
    }
    let focused = pane_group.as_ref(ctx).focused_pane_id(ctx);
    if let Some(source_id) = current_input_source_id(ctx) {
        remember_source(ctx, focused, &source_id);
    }
}

#[cfg(test)]
#[path = "input_source_policy_tests.rs"]
mod tests;
