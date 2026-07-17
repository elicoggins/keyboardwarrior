// Player-tunable options: the global setting statics and the settings-
// screen rows that adjust them.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

use crate::audio::AudioEngine;
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

pub static SUSTAINS: AtomicBool = AtomicBool::new(true);

pub fn sustains_on() -> bool {
    SUSTAINS.load(Ordering::Relaxed)
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
    Sustains,
    Speed,
    Volume,
    Calibrate,
}

/// The rows currently on screen: the practice key filters only appear while
/// the text mode is PRACTICE, indented under it.
pub fn settings_rows() -> Vec<SettingRow> {
    let mut rows = vec![SettingRow::TextMode];
    if text_mode() == TextMode::Practice {
        rows.extend([
            SettingRow::PracLeft,
            SettingRow::PracRight,
            SettingRow::PracTop,
            SettingRow::PracHome,
            SettingRow::PracBottom,
            SettingRow::PracPunct,
        ]);
    }
    rows.extend([
        SettingRow::Theme,
        SettingRow::Sustains,
        SettingRow::Speed,
        SettingRow::Volume,
        SettingRow::Calibrate,
    ]);
    rows
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
            SettingRow::Sustains => "sustains",
            SettingRow::Speed => "speed",
            SettingRow::Volume => "volume",
            SettingRow::Calibrate => "calibrate",
        }
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
            SettingRow::Sustains => on_off(&SUSTAINS).into(),
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
            SettingRow::Sustains => flip(&SUSTAINS),
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
            SettingRow::Sustains => "hold long notes for bonus score",
            SettingRow::Speed => "how long notes stay on the highway",
            SettingRow::Volume => "master volume - also -/+ from anywhere",
            SettingRow::Calibrate => "ENTER: tap along to measure your keyboard latency",
        }
    }
}
