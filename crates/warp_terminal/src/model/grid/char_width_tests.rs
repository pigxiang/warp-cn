use serial_test::serial;

use super::*;

struct ModeGuard(CharWidthMode);

impl ModeGuard {
    fn new() -> Self {
        let previous = char_width_mode();
        set_char_width_mode(CharWidthMode::Off);
        Self(previous)
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        set_char_width_mode(self.0);
    }
}

fn width_with(mode: CharWidthMode, c: char) -> Option<usize> {
    set_char_width_mode(mode);
    display_width(c)
}

#[test]
#[serial]
fn off_mode_preserves_unicode_width_default() {
    let _guard = ModeGuard::new();
    assert_eq!(width_with(CharWidthMode::Off, '①'), Some(1));
    assert_eq!(width_with(CharWidthMode::Off, 'α'), Some(1));
    assert_eq!(width_with(CharWidthMode::Off, '─'), Some(1));
    assert_eq!(width_with(CharWidthMode::Off, '你'), Some(2));
    assert_eq!(width_with(CharWidthMode::Off, '\u{301}'), Some(0));
    assert_eq!(width_with(CharWidthMode::Off, '\u{7}'), None);
}

#[test]
#[serial]
fn cjk_symbols_widens_only_fullwidth_rendered_symbols() {
    let _guard = ModeGuard::new();
    for c in ['①', '⑩', '⑳', 'Ⓐ', '⓪', '㊙', '〇', '℃', '℉', '№'] {
        assert_eq!(width_with(CharWidthMode::CjkSymbols, c), Some(2), "{c}");
    }
    for c in ['─', '│', '█', '→', '▲', 'α', 'β', 'ö'] {
        assert_eq!(width_with(CharWidthMode::CjkSymbols, c), Some(1), "{c}");
    }
    assert_eq!(width_with(CharWidthMode::CjkSymbols, '你'), Some(2));
    assert_eq!(width_with(CharWidthMode::CjkSymbols, '\u{301}'), Some(0));
}

#[test]
#[serial]
fn all_ambiguous_widens_entire_east_asian_ambiguous_class() {
    let _guard = ModeGuard::new();
    for c in ['①', 'α', 'Α', 'à', '─', '→', '▲', '℃', '⓪'] {
        assert_eq!(width_with(CharWidthMode::AllAmbiguous, c), Some(2), "{c}");
    }
    assert_eq!(width_with(CharWidthMode::AllAmbiguous, '☁'), Some(1));
    assert_eq!(width_with(CharWidthMode::AllAmbiguous, '你'), Some(2));
    assert_eq!(width_with(CharWidthMode::AllAmbiguous, '\u{301}'), Some(0));
}

#[test]
#[serial]
fn range_boundaries_are_inclusive() {
    let _guard = ModeGuard::new();
    // U+245F and U+2500 flank the U+2460..=U+24FF whitelist range.
    assert_eq!(width_with(CharWidthMode::CjkSymbols, '\u{245F}'), Some(1));
    assert_eq!(width_with(CharWidthMode::CjkSymbols, '\u{24FF}'), Some(2));
    assert_eq!(width_with(CharWidthMode::CjkSymbols, '\u{2500}'), Some(1));
}
