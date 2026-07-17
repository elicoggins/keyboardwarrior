// Color themes: lane, accent, and status palettes, plus the small color
// helpers everything paints with.

use std::sync::atomic::{AtomicUsize, Ordering};

use macroquad::prelude::Color;

pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub lane: [Color; 4],
    pub accent: Color,    // star power, combo multiplier, highlights
    pub secondary: Color, // subtitles, progress, GREAT judgement
    pub good: Color,      // GOOD judgement
    pub miss: Color,
}

pub const THEMES: [Theme; 3] = [
    // Black / slate / orange: near-black base, two ember lanes, two slate
    Theme {
        name: "EMBER",
        bg: Color::new(0.043, 0.045, 0.052, 1.0),
        lane: [
            Color::new(0.96, 0.62, 0.12, 1.0), // amber
            Color::new(0.98, 0.45, 0.10, 1.0), // orange
            Color::new(0.34, 0.48, 0.72, 1.0), // steel blue
            Color::new(0.85, 0.88, 0.92, 1.0), // pale slate
        ],
        accent: Color::new(0.99, 0.72, 0.25, 1.0),
        secondary: Color::new(0.64, 0.70, 0.78, 1.0),
        good: Color::new(0.72, 0.70, 0.66, 1.0),
        miss: Color::new(0.94, 0.33, 0.25, 1.0),
    },
    // Deep indigo night with jewel-tone lanes
    Theme {
        name: "MIDNIGHT",
        bg: Color::new(0.055, 0.058, 0.098, 1.0),
        lane: [
            Color::new(0.18, 0.83, 0.75, 1.0), // teal
            Color::new(0.65, 0.55, 0.98, 1.0), // violet
            Color::new(0.49, 0.83, 0.99, 1.0), // sky
            Color::new(0.98, 0.44, 0.52, 1.0), // rose
        ],
        accent: Color::new(0.99, 0.83, 0.30, 1.0),
        secondary: Color::new(0.45, 0.80, 1.00, 1.0),
        good: Color::new(0.75, 0.75, 0.80, 1.0),
        miss: Color::new(1.00, 0.33, 0.33, 1.0),
    },
    // Dark evergreen with northern-lights lanes
    Theme {
        name: "AURORA",
        bg: Color::new(0.035, 0.062, 0.055, 1.0),
        lane: [
            Color::new(0.43, 0.91, 0.72, 1.0), // mint
            Color::new(0.40, 0.88, 0.98, 1.0), // cyan
            Color::new(0.77, 0.71, 0.99, 1.0), // lilac
            Color::new(0.99, 0.86, 0.55, 1.0), // sand
        ],
        accent: Color::new(0.96, 0.78, 0.42, 1.0),
        secondary: Color::new(0.45, 0.86, 0.83, 1.0),
        good: Color::new(0.70, 0.78, 0.75, 1.0),
        miss: Color::new(1.00, 0.42, 0.42, 1.0),
    },
];

pub static THEME_IDX: AtomicUsize = AtomicUsize::new(0);

pub fn th() -> &'static Theme {
    &THEMES[THEME_IDX.load(Ordering::Relaxed) % THEMES.len()]
}

/// A theme color at a given alpha.
pub fn wa(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Blend two colors.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
