use std::sync::Arc;
use std::time::Duration;

use warpui::r#async::Timer;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::server::server_api::ServerApiProvider;
use crate::server::server_api::tui_onboarding::{
    TuiOnboardingClient, TuiOnboardingMarkersSnapshot,
};

const MARKER_LOAD_TIMEOUT: Duration = Duration::from_secs(3);

/// Account-scoped, monotonic TUI onboarding markers resolved from the account
/// snapshot; consumption is tracked for the current process only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiOnboardingMarker {
    FirstZeroState,
    FirstCreditGate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TuiOnboardingMarkersState {
    Loading,
    Ready {
        first_zero_state_available: bool,
        first_credit_gate_available: bool,
    },
}

/// Events emitted as the current account's marker snapshot is invalidated and resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiOnboardingMarkersEvent {
    Loading,
    Ready,
}

/// Dedicated cloud-preference model for the TUI's once-per-account surfaces.
pub struct TuiOnboardingMarkers {
    state: TuiOnboardingMarkersState,
    load_generation: u64,
    onboarding_client: Option<Arc<dyn TuiOnboardingClient>>,
    load_timeout: Duration,
}

impl TuiOnboardingMarkers {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let onboarding_client = ServerApiProvider::as_ref(ctx).get_tui_onboarding_client();
        let _ = ctx;
        Self {
            state: TuiOnboardingMarkersState::Loading,
            load_generation: 0,
            onboarding_client: Some(onboarding_client),
            load_timeout: MARKER_LOAD_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn new_loading_for_test(
        onboarding_client: Arc<dyn TuiOnboardingClient>,
        load_timeout: Duration,
    ) -> Self {
        Self {
            state: TuiOnboardingMarkersState::Loading,
            load_generation: 0,
            onboarding_client: Some(onboarding_client),
            load_timeout,
        }
    }

    /// Starts a fresh, account-scoped load. Terminal creation never waits for
    /// this request; consumers keep one-time UI hidden until
    /// [`TuiOnboardingMarkersEvent::Ready`].
    pub fn load_current_account(&mut self, ctx: &mut ModelContext<Self>) {
        self.state = TuiOnboardingMarkersState::Loading;
        self.load_generation = self.load_generation.wrapping_add(1);
        ctx.emit(TuiOnboardingMarkersEvent::Loading);
        let load_generation = self.load_generation;
        let onboarding_client = self
            .onboarding_client
            .clone()
            .expect("production TUI onboarding marker model must have an API client");
        let load_timeout = self.load_timeout;
        ctx.spawn(
            load_markers_with_timeout(onboarding_client, load_timeout),
            move |markers, result, ctx| {
                markers.handle_load_result(load_generation, result, ctx);
            },
        );
        ctx.notify();
    }

    /// Invalidates the previous account snapshot before a signed-out terminal
    /// can be created for the next browser authentication flow.
    pub fn reset_for_account_transition(&mut self, ctx: &mut ModelContext<Self>) {
        self.state = TuiOnboardingMarkersState::Loading;
        self.load_generation = self.load_generation.wrapping_add(1);
        ctx.emit(TuiOnboardingMarkersEvent::Loading);
        ctx.notify();
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state, TuiOnboardingMarkersState::Ready { .. })
    }

    /// Consumes a marker for this account snapshot. Returning `false` means the
    /// marker was loading or had already been consumed in this process/account
    /// snapshot.
    pub fn consume(&mut self, marker: TuiOnboardingMarker, ctx: &mut ModelContext<Self>) -> bool {
        if !self.take_available(marker) {
            return false;
        }
        ctx.notify();
        true
    }

    fn resolve_from_snapshot(
        &mut self,
        snapshot: TuiOnboardingMarkersSnapshot,
        ctx: &mut ModelContext<Self>,
    ) {
        self.state = TuiOnboardingMarkersState::Ready {
            first_zero_state_available: !snapshot.first_zero_state_shown,
            first_credit_gate_available: !snapshot.first_credit_gate_shown,
        };
        ctx.emit(TuiOnboardingMarkersEvent::Ready);
        ctx.notify();
    }
    fn resolve_unavailable(&mut self, ctx: &mut ModelContext<Self>) {
        self.state = TuiOnboardingMarkersState::Ready {
            first_zero_state_available: false,
            first_credit_gate_available: false,
        };
        ctx.emit(TuiOnboardingMarkersEvent::Ready);
        ctx.notify();
    }
    fn handle_load_result(
        &mut self,
        load_generation: u64,
        result: MarkerLoadResult,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.load_generation != load_generation {
            return;
        }
        match result {
            MarkerLoadResult::Loaded(Ok(snapshot)) => {
                self.resolve_from_snapshot(snapshot, ctx);
            }
            MarkerLoadResult::Loaded(Err(error)) => {
                log::warn!(
                    "Unable to load TUI onboarding markers; continuing without one-time onboarding surfaces: {error:#}"
                );
                self.resolve_unavailable(ctx);
            }
            MarkerLoadResult::TimedOut => {
                log::warn!(
                    "Timed out loading TUI onboarding markers; continuing without one-time onboarding surfaces"
                );
                self.resolve_unavailable(ctx);
            }
        }
    }

    fn take_available(&mut self, marker: TuiOnboardingMarker) -> bool {
        let TuiOnboardingMarkersState::Ready {
            first_zero_state_available,
            first_credit_gate_available,
        } = &mut self.state
        else {
            return false;
        };
        let available = match marker {
            TuiOnboardingMarker::FirstZeroState => first_zero_state_available,
            TuiOnboardingMarker::FirstCreditGate => first_credit_gate_available,
        };
        std::mem::take(available)
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn new_ready_for_test(
        first_zero_state_available: bool,
        first_credit_gate_available: bool,
    ) -> Self {
        Self {
            state: TuiOnboardingMarkersState::Ready {
                first_zero_state_available,
                first_credit_gate_available,
            },
            load_generation: 0,
            onboarding_client: None,
            load_timeout: MARKER_LOAD_TIMEOUT,
        }
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn set_ready_for_test(
        &mut self,
        first_zero_state_available: bool,
        first_credit_gate_available: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.state = TuiOnboardingMarkersState::Ready {
            first_zero_state_available,
            first_credit_gate_available,
        };
        ctx.emit(TuiOnboardingMarkersEvent::Ready);
        ctx.notify();
    }
}

impl Entity for TuiOnboardingMarkers {
    type Event = TuiOnboardingMarkersEvent;
}
impl SingletonEntity for TuiOnboardingMarkers {}

enum MarkerLoadResult {
    Loaded(anyhow::Result<TuiOnboardingMarkersSnapshot>),
    TimedOut,
}

async fn load_markers_with_timeout(
    onboarding_client: Arc<dyn TuiOnboardingClient>,
    timeout: Duration,
) -> MarkerLoadResult {
    let load = onboarding_client.get_tui_onboarding_markers();
    let timeout = Timer::after(timeout);
    futures::pin_mut!(load);
    futures::pin_mut!(timeout);
    match futures::future::select(load, timeout).await {
        futures::future::Either::Left((result, _)) => MarkerLoadResult::Loaded(result),
        futures::future::Either::Right(_) => MarkerLoadResult::TimedOut,
    }
}

#[cfg(test)]
#[path = "tui_onboarding_markers_tests.rs"]
mod tests;
