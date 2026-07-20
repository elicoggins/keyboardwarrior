// Player-tunable options: the global setting statics and the settings-
// screen rows that adjust them.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

use crate::audio::AudioEngine;
use crate::controls::Cap;
use crate::theme::{th, THEMES, THEME_IDX};
use crate::words::{
    text_mode, TextMode, PRAC_BOTTOM, PRAC_HOME, PRAC_LEFT, PRAC_PUNCT, PRAC_RIGHT, PRAC_TOP,
    TEXT_MODES, TEXT_MODE_IDX,
};

// How long a note is visible before it reaches the strike line — the board
// scroll speed. V cycles the presets in the menu; shorter time = faster.
pub const SPEEDS: [(&str, f64); 4] =
    [("SLOW", 2.6), ("NORMAL", 2.0), ("FAST", 1.5), ("TURBO", 1.1)];
pub static SPEED_IDX: AtomicUsize = AtomicUsize::new(1);

pub fn approach() -> f64 {
    SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].1
}
// Calibration metronome period (120 BPM)
pub const CALIB_PERIOD: f64 = 0.5;

// Input latency compensation measured on the calibration screen, in ms.
// Subtracted from the clock when judging keypresses (visuals stay raw).
pub static CALIB_MS: AtomicI64 = AtomicI64::new(0);

pub fn calib_offset() -> f64 {
    CALIB_MS.load(Ordering::Relaxed) as f64 / 1000.0
}

/// One adjustable row on the settings screen.
#[derive(Clone, Copy, PartialEq)]
pub enum SettingRow {
    TextMode,
    PracLeft,
    PracRight,
    PracTop,
    PracHome,
    PracBottom,
    PracPunct,
    Theme,
    Speed,
    Volume,
    Calibrate,
}

/// One line of the settings screen: a section heading, or a row that adjusts
/// something. Headings are skipped by the cursor — `settings_rows` returns
/// only what's selectable, `settings_lines` returns what's drawn.
#[derive(Clone, Copy, PartialEq)]
pub enum SettingLine {
    Section(&'static str),
    Row(SettingRow),
}

/// Everything the settings screen draws, in order.
///
/// Grouped by what a player is actually trying to change, using the game's own
/// vocabulary: what the notes say, how the highway behaves, and how it sounds
/// and lines up. The practice key filters only appear while the text mode is
/// PRACTICE, indented under it.
pub fn settings_lines() -> Vec<SettingLine> {
    use SettingLine::{Row, Section};
    let mut lines = vec![Section("CONTENT"), Row(SettingRow::TextMode)];
    if text_mode() == TextMode::Practice {
        lines.extend(
            [
                SettingRow::PracLeft,
                SettingRow::PracRight,
                SettingRow::PracTop,
                SettingRow::PracHome,
                SettingRow::PracBottom,
                SettingRow::PracPunct,
            ]
            .map(Row),
        );
    }
    lines.extend([
        Section("HIGHWAY"),
        Row(SettingRow::Speed),
        Row(SettingRow::Theme),
        Section("AUDIO & TIMING"),
        Row(SettingRow::Volume),
        Row(SettingRow::Calibrate),
    ]);
    lines
}

/// Just the selectable rows, in screen order.
pub fn settings_rows() -> Vec<SettingRow> {
    settings_lines()
        .into_iter()
        .filter_map(|l| match l {
            SettingLine::Row(r) => Some(r),
            SettingLine::Section(_) => None,
        })
        .collect()
}

pub fn cycle(idx: &AtomicUsize, n: usize, dir: i32) {
    let i = idx.load(Ordering::Relaxed) as i32 + dir;
    idx.store(i.rem_euclid(n as i32) as usize, Ordering::Relaxed);
}

pub fn flip(b: &AtomicBool) {
    b.store(!b.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn on_off(b: &AtomicBool) -> &'static str {
    if b.load(Ordering::Relaxed) {
        "ON"
    } else {
        "OFF"
    }
}

impl SettingRow {
    pub fn label(self) -> &'static str {
        match self {
            SettingRow::TextMode => "text mode",
            SettingRow::PracLeft => "left hand",
            SettingRow::PracRight => "right hand",
            SettingRow::PracTop => "top row",
            SettingRow::PracHome => "home row",
            SettingRow::PracBottom => "bottom row",
            SettingRow::PracPunct => "punctuation",
            SettingRow::Theme => "theme",
            SettingRow::Speed => "speed",
            SettingRow::Volume => "volume",
            SettingRow::Calibrate => "calibrate",
        }
    }

    /// The menu shortcut that changes this row without opening settings, if
    /// it has one. Drawn beside the row, so the two surfaces teach each other:
    /// whoever finds a setting here also learns the key that skips the trip.
    pub fn hotkey(self) -> Option<Cap> {
        match self {
            SettingRow::TextMode => Some(Cap::Txt("M")),
            SettingRow::Theme => Some(Cap::Txt("T")),
            SettingRow::Speed => Some(Cap::Txt("V")),
            SettingRow::Calibrate => Some(Cap::Txt("C")),
            SettingRow::Volume => Some(Cap::Pair("-", "+")),
            _ => None,
        }
    }

    /// Whether this row is an on/off toggle and which way it's set, so a
    /// state can be read off its color without parsing the word.
    pub fn toggle(self) -> Option<bool> {
        let b = match self {
            SettingRow::PracLeft => &PRAC_LEFT,
            SettingRow::PracRight => &PRAC_RIGHT,
            SettingRow::PracTop => &PRAC_TOP,
            SettingRow::PracHome => &PRAC_HOME,
            SettingRow::PracBottom => &PRAC_BOTTOM,
            SettingRow::PracPunct => &PRAC_PUNCT,
            _ => return None,
        };
        Some(b.load(Ordering::Relaxed))
    }

    pub fn indented(self) -> bool {
        matches!(
            self,
            SettingRow::PracLeft
                | SettingRow::PracRight
                | SettingRow::PracTop
                | SettingRow::PracHome
                | SettingRow::PracBottom
                | SettingRow::PracPunct
        )
    }

    pub fn value(self, engine: &AudioEngine) -> String {
        match self {
            SettingRow::TextMode => {
                TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].1.to_string()
            }
            SettingRow::PracLeft => on_off(&PRAC_LEFT).into(),
            SettingRow::PracRight => on_off(&PRAC_RIGHT).into(),
            SettingRow::PracTop => on_off(&PRAC_TOP).into(),
            SettingRow::PracHome => on_off(&PRAC_HOME).into(),
            SettingRow::PracBottom => on_off(&PRAC_BOTTOM).into(),
            SettingRow::PracPunct => on_off(&PRAC_PUNCT).into(),
            SettingRow::Theme => th().name.to_string(),
            SettingRow::Speed => {
                SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].0.to_string()
            }
            SettingRow::Volume => format!("{:.0}%", engine.master() * 100.0),
            SettingRow::Calibrate => format!("{:+} ms", CALIB_MS.load(Ordering::Relaxed)),
        }
    }

    pub fn adjust(self, dir: i32, engine: &AudioEngine) {
        match self {
            SettingRow::TextMode => cycle(&TEXT_MODE_IDX, TEXT_MODES.len(), dir),
            SettingRow::PracLeft => flip(&PRAC_LEFT),
            SettingRow::PracRight => flip(&PRAC_RIGHT),
            SettingRow::PracTop => flip(&PRAC_TOP),
            SettingRow::PracHome => flip(&PRAC_HOME),
            SettingRow::PracBottom => flip(&PRAC_BOTTOM),
            SettingRow::PracPunct => flip(&PRAC_PUNCT),
            SettingRow::Theme => cycle(&THEME_IDX, THEMES.len(), dir),
            SettingRow::Speed => cycle(&SPEED_IDX, SPEEDS.len(), dir),
            SettingRow::Volume => {
                engine.set_master(((engine.master() + 0.05 * dir as f32) * 20.0).round() / 20.0);
            }
            SettingRow::Calibrate => {} // ENTER opens the metronome instead
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            SettingRow::TextMode => match text_mode() {
                TextMode::Words => "phrases become real words sized to the beat",
                TextMode::WordsFixed => "words, but a song + difficulty always deals the same ones",
                TextMode::Sentences => "coherent sentences streamed letter by letter",
                TextMode::Dfjk => "four keys, four lanes - any key in a lane's zone counts",
                TextMode::Practice => "random letters - tune which keys appear below",
            },
            SettingRow::PracLeft => "letters typed by the left hand",
            SettingRow::PracRight => "letters typed by the right hand",
            SettingRow::PracTop => "the qwerty row",
            SettingRow::PracHome => "the asdf row",
            SettingRow::PracBottom => "the zxcv row",
            SettingRow::PracPunct => "comma, period, apostrophe - shift is never needed",
            SettingRow::Theme => "lane and accent colors",
            SettingRow::Speed => "how long notes stay on the highway",
            SettingRow::Volume => "master volume - also -/+ from anywhere",
            SettingRow::Calibrate => "ENTER: tap along to measure your keyboard latency",
        }
    }
}
