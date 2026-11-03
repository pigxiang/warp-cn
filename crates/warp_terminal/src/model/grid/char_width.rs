//! Cell-width determination for terminal grid input.
//!
//! `unicode-width` renders East Asian Ambiguous characters one cell wide,
//! which garbles CJK text when the font draws those glyphs fullwidth.
//! `CjkSymbols` widens the symbols CJK monospace fonts render fullwidth
//! (enclosed alphanumerics, circled CJK symbols, etc.); `AllAmbiguous` widens
//! every East Asian Ambiguous character, matching kitty's `ambiguous-width = 2`.

use std::sync::atomic::{AtomicU8, Ordering};

use unicode_width::UnicodeWidthChar as _;

/// How East Asian Ambiguous characters occupy the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharWidthMode {
    /// Ambiguous characters occupy one cell (the upstream default).
    Off,
    /// Only characters CJK monospace fonts render fullwidth occupy two cells.
    CjkSymbols,
    /// Every assigned East Asian Ambiguous character occupies two cells.
    AllAmbiguous,
}

impl CharWidthMode {
    fn encode(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::CjkSymbols => 1,
            Self::AllAmbiguous => 2,
        }
    }

    fn decode(value: u8) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::CjkSymbols,
            2 => Self::AllAmbiguous,
            other => panic!("unknown CharWidthMode discriminant: {other}"),
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_char_width_mode(mode: CharWidthMode) {
    MODE.store(mode.encode(), Ordering::Relaxed);
}

pub fn char_width_mode() -> CharWidthMode {
    CharWidthMode::decode(MODE.load(Ordering::Relaxed))
}

/// Returns the number of grid cells `c` occupies, or `None` if `c` is not
/// rendered (control characters).
pub fn display_width(c: char) -> Option<usize> {
    let width = c.width()?;
    if width == 1 {
        let wide = match char_width_mode() {
            CharWidthMode::Off => false,
            CharWidthMode::CjkSymbols => in_ranges(CJK_SYMBOLS_WIDE_RANGES, c),
            // Union with the whitelist keeps this mode a strict superset of
            // `CjkSymbols` for characters UCD classifies as neither A nor W
            // (e.g. U+24EA, fullwidth in CJK fonts).
            CharWidthMode::AllAmbiguous => {
                in_ranges(AMBIGUOUS_WIDE_RANGES, c) || in_ranges(CJK_SYMBOLS_WIDE_RANGES, c)
            }
        };
        if wide {
            return Some(2);
        }
    }
    Some(width)
}

fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    let cp = c as u32;
    let idx = ranges.partition_point(|&(lo, _)| lo <= cp);
    idx > 0 && cp <= ranges[idx - 1].1
}

/// Font-driven whitelist for `CjkSymbols`: characters CJK monospace fonts
/// (Sarasa, Noto Sans Mono CJK, Source Han) draw fullwidth regardless of their
/// strict East-Asian-Ambiguous classification. Box-drawing, block elements,
/// arrows and geometric shapes are deliberately excluded: fonts draw them
/// halfwidth and the whole TUI ecosystem computes them as one cell.
const CJK_SYMBOLS_WIDE_RANGES: &[(u32, u32)] = &[
    (0x2103, 0x2103),
    (0x2109, 0x2109),
    (0x2116, 0x2116),
    (0x2460, 0x24FF),
    (0x3007, 0x3007),
    (0x3200, 0x32FF),
    (0x3300, 0x33FF),
    (0x1F200, 0x1F2FF),
];

/// Every assigned code point with `East_Asian_Width=A` in Unicode 15.1.0
/// (the version of the crate's tables). Generated from
/// EastAsianWidth-15.1.0.txt intersected with assigned code points in
/// UnicodeData-15.1.0.txt, with adjacent ranges merged; includes the Private
/// Use Areas, which breaks powerline-style prompt symbols - the accepted cost
/// of this mode.
const AMBIGUOUS_WIDE_RANGES: &[(u32, u32)] = &[
    (0x00A1, 0x00A1),
    (0x00A4, 0x00A4),
    (0x00A7, 0x00A8),
    (0x00AA, 0x00AA),
    (0x00AD, 0x00AE),
    (0x00B0, 0x00B4),
    (0x00B6, 0x00BA),
    (0x00BC, 0x00BF),
    (0x00C6, 0x00C6),
    (0x00D0, 0x00D0),
    (0x00D7, 0x00D8),
    (0x00DE, 0x00E1),
    (0x00E6, 0x00E6),
    (0x00E8, 0x00EA),
    (0x00EC, 0x00ED),
    (0x00F0, 0x00F0),
    (0x00F2, 0x00F3),
    (0x00F7, 0x00FA),
    (0x00FC, 0x00FC),
    (0x00FE, 0x00FE),
    (0x0101, 0x0101),
    (0x0111, 0x0111),
    (0x0113, 0x0113),
    (0x011B, 0x011B),
    (0x0126, 0x0127),
    (0x012B, 0x012B),
    (0x0131, 0x0133),
    (0x0138, 0x0138),
    (0x013F, 0x0142),
    (0x0144, 0x0144),
    (0x0148, 0x014B),
    (0x014D, 0x014D),
    (0x0152, 0x0153),
    (0x0166, 0x0167),
    (0x016B, 0x016B),
    (0x01CE, 0x01CE),
    (0x01D0, 0x01D0),
    (0x01D2, 0x01D2),
    (0x01D4, 0x01D4),
    (0x01D6, 0x01D6),
    (0x01D8, 0x01D8),
    (0x01DA, 0x01DA),
    (0x01DC, 0x01DC),
    (0x0251, 0x0251),
    (0x0261, 0x0261),
    (0x02C4, 0x02C4),
    (0x02C7, 0x02C7),
    (0x02C9, 0x02CB),
    (0x02CD, 0x02CD),
    (0x02D0, 0x02D0),
    (0x02D8, 0x02DB),
    (0x02DD, 0x02DD),
    (0x02DF, 0x02DF),
    (0x0300, 0x036F),
    (0x0391, 0x03A1),
    (0x03A3, 0x03A9),
    (0x03B1, 0x03C1),
    (0x03C3, 0x03C9),
    (0x0401, 0x0401),
    (0x0410, 0x044F),
    (0x0451, 0x0451),
    (0x2010, 0x2010),
    (0x2013, 0x2016),
    (0x2018, 0x2019),
    (0x201C, 0x201D),
    (0x2020, 0x2022),
    (0x2024, 0x2027),
    (0x2030, 0x2030),
    (0x2032, 0x2033),
    (0x2035, 0x2035),
    (0x203B, 0x203B),
    (0x203E, 0x203E),
    (0x2074, 0x2074),
    (0x207F, 0x207F),
    (0x2081, 0x2084),
    (0x20AC, 0x20AC),
    (0x2103, 0x2103),
    (0x2105, 0x2105),
    (0x2109, 0x2109),
    (0x2113, 0x2113),
    (0x2116, 0x2116),
    (0x2121, 0x2122),
    (0x2126, 0x2126),
    (0x212B, 0x212B),
    (0x2153, 0x2154),
    (0x215B, 0x215E),
    (0x2160, 0x216B),
    (0x2170, 0x2179),
    (0x2189, 0x2189),
    (0x2190, 0x2199),
    (0x21B8, 0x21B9),
    (0x21D2, 0x21D2),
    (0x21D4, 0x21D4),
    (0x21E7, 0x21E7),
    (0x2200, 0x2200),
    (0x2202, 0x2203),
    (0x2207, 0x2208),
    (0x220B, 0x220B),
    (0x220F, 0x220F),
    (0x2211, 0x2211),
    (0x2215, 0x2215),
    (0x221A, 0x221A),
    (0x221D, 0x2220),
    (0x2223, 0x2223),
    (0x2225, 0x2225),
    (0x2227, 0x222C),
    (0x222E, 0x222E),
    (0x2234, 0x2237),
    (0x223C, 0x223D),
    (0x2248, 0x2248),
    (0x224C, 0x224C),
    (0x2252, 0x2252),
    (0x2260, 0x2261),
    (0x2264, 0x2267),
    (0x226A, 0x226B),
    (0x226E, 0x226F),
    (0x2282, 0x2283),
    (0x2286, 0x2287),
    (0x2295, 0x2295),
    (0x2299, 0x2299),
    (0x22A5, 0x22A5),
    (0x22BF, 0x22BF),
    (0x2312, 0x2312),
    (0x2460, 0x24E9),
    (0x24EB, 0x254B),
    (0x2550, 0x2573),
    (0x2580, 0x258F),
    (0x2592, 0x2595),
    (0x25A0, 0x25A1),
    (0x25A3, 0x25A9),
    (0x25B2, 0x25B3),
    (0x25B6, 0x25B7),
    (0x25BC, 0x25BD),
    (0x25C0, 0x25C1),
    (0x25C6, 0x25C8),
    (0x25CB, 0x25CB),
    (0x25CE, 0x25D1),
    (0x25E2, 0x25E5),
    (0x25EF, 0x25EF),
    (0x2605, 0x2606),
    (0x2609, 0x2609),
    (0x260E, 0x260F),
    (0x261C, 0x261C),
    (0x261E, 0x261E),
    (0x2640, 0x2640),
    (0x2642, 0x2642),
    (0x2660, 0x2661),
    (0x2663, 0x2665),
    (0x2667, 0x266A),
    (0x266C, 0x266D),
    (0x266F, 0x266F),
    (0x269E, 0x269F),
    (0x26BF, 0x26BF),
    (0x26C6, 0x26CD),
    (0x26CF, 0x26D3),
    (0x26D5, 0x26E1),
    (0x26E3, 0x26E3),
    (0x26E8, 0x26E9),
    (0x26EB, 0x26F1),
    (0x26F4, 0x26F4),
    (0x26F6, 0x26F9),
    (0x26FB, 0x26FC),
    (0x26FE, 0x26FF),
    (0x273D, 0x273D),
    (0x2776, 0x277F),
    (0x2B56, 0x2B59),
    (0x3248, 0x324F),
    (0xE000, 0xF8FF),
    (0xFE00, 0xFE0F),
    (0xFFFD, 0xFFFD),
    (0x1F100, 0x1F10A),
    (0x1F110, 0x1F12D),
    (0x1F130, 0x1F169),
    (0x1F170, 0x1F18D),
    (0x1F18F, 0x1F190),
    (0x1F19B, 0x1F1AC),
    (0xE0100, 0xE01EF),
    (0xF0000, 0xFFFFD),
    (0x100000, 0x10FFFD),
];

#[cfg(test)]
#[path = "char_width_tests.rs"]
mod tests;
