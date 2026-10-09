pub mod ai_agent_tasks;
pub mod settings;
pub mod terminal;
mod virtual_fs;

pub use terminal::add_window_with_terminal;
pub use virtual_fs::{Stub, VirtualFS};
pub use warp_terminal::test_util::mock_blockgrid;

/// warp-cn fork: pins the UI locale to English for tests that assert on
/// user-visible strings. Without an initialized bundle the fork's `t!()` renders
/// `{key}` placeholders, so upstream assertions on English copy fail.
pub fn use_english_locale() {
    let _ = warp_i18n::init(warp_i18n::Locale::En);
    warp_i18n::set_locale(warp_i18n::Locale::En);
}

macro_rules! assert_eventually {
    ($cond:expr_2021, $($arg:tt)+) => {
        $crate::test_util::assert_eventually!(20 => $cond, $($arg)+);
    };
    // Run the condition up to ticks times, yielding to the executor in between.  If it does
    // not become true, this panics with the provided format string + args.
    ($ticks:literal => $cond:expr_2021, $($arg:tt)+) => {{
        let mut pass = false;
        for _ in 0..$ticks {
            if $cond {
                pass = true;
                break;
            }
            warpui::r#async::Timer::after(std::time::Duration::from_millis(5)).await;
        }
        if !pass {
            panic!("{}", format_args!($($arg)+));
        }
    }};
}
pub(crate) use assert_eventually;
