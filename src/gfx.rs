// Text and overlay drawing: the quantized pixel-font helpers, centered/
// fitted text, and the F1 frame-time graph. Also the one layout scale every
// screen multiplies its constants by.

use macroquad::prelude::*;

use crate::theme::{th, wa};

/// The viewport every layout constant in this codebase is written against.
/// Sizes and offsets are authored as if the window were exactly this, then
/// multiplied by `ui()` to land on the real one.
pub const REF_W: f32 = 1100.0;
pub const REF_H: f32 = 800.0;

/// Global layout scale: how much bigger (or smaller) the window is than the
/// reference viewport, taking whichever axis is tighter so growing on one
/// never overflows the other.
///
/// Everything drawn is a multiple of this, and two useful properties fall
/// out of that. Fixed chrome shrinks along with the space it sits in, so
/// lists and gutters keep their row counts on a short window instead of
/// collapsing to nothing; and the highway grows on a big display instead of
/// stranding a fixed-width ribbon in the middle of a 4K panel.
///
/// Clamped at both ends: below ~0.6 the pixel font stops resolving, and past
/// ~2.2 a very large display just gets comically heavy type.
pub fn ui() -> f32 {
    (screen_width() / REF_W).min(screen_height() / REF_H).clamp(0.6, 2.2)
}

// The built-in pixel font, everywhere — menu, HUD, gems, words.
//
// macroquad rasterizes glyphs per (character, pixel size) into one shared
// atlas, and any frame that adds a glyph re-uploads the entire atlas texture
// (and occasionally doubles it). Several text sizes here animate continuously
// — the word queue, the combo counter, the menu wheel — which would mint new
// pixel sizes almost every frame and stutter. So glyphs are only rasterized
// at SIZE_STEP-quantized sizes; font_scale closes the gap by scaling the
// cached quad, which costs nothing.
const SIZE_STEP: f32 = 4.0;

/// Largest size ever rasterized; anything bigger scales a cached quad instead.
///
/// This ceiling is deliberately independent of `ui()`. macroquad's atlas grows
/// by *doubling* and never shrinks, so if the rasterized size range chased the
/// layout scale, a large display would walk it up the ladder — and at 32768²
/// the texture's byte size is exactly 2^32, which wraps miniquad's u32 size
/// calculation to zero and trips an assert inside `Texture::new`. Scaling a
/// cached quad is free and, for a pixel font, visually indistinguishable.
const MAX_RASTER: f32 = 160.0;

fn qsize(size: f32) -> (u16, f32) {
    let bucket = (size / SIZE_STEP).ceil().clamp(1.0, MAX_RASTER / SIZE_STEP) * SIZE_STEP;
    (bucket as u16, size / bucket)
}

pub fn dtext(t: &str, x: f32, y: f32, size: f32, color: Color) {
    let (font_size, font_scale) = qsize(size);
    draw_text_ex(t, x, y, TextParams { font_size, font_scale, color, ..Default::default() });
}

pub fn msize(t: &str, size: f32) -> TextDimensions {
    let (font_size, font_scale) = qsize(size);
    measure_text(t, None, font_size, font_scale)
}

/// Rasterize every glyph the game can draw once, at startup, so the atlas
/// never grows or re-uploads mid-song. (measure_text caches glyphs too.)
///
/// Covers every size at every layout scale, so it needs no `ui()` input and
/// never has to run again on a resize.
pub fn prewarm_glyphs() {
    // Printable ASCII plus the one non-ASCII glyph the UI draws — the middot
    // separator. (The bundled font has no em-dash/ellipsis glyph, so those must
    // not be drawn; anything outside this set renders as a missing-glyph box.)
    let charset: String = (' '..='~').chain(['·']).collect();
    let mut bucket = SIZE_STEP;
    while bucket <= 96.0 {
        measure_text(&charset, None, bucket as u16, 1.0);
        bucket += SIZE_STEP;
    }
    // Past 96 the only things drawn are headings, the count-in, the combo and
    // the results grade — letters and digits, never punctuation. `ui()` tops
    // out at 2.2, and the largest body text (the word queue at 44) only just
    // crosses 96 there, so restricting the big buckets to this set costs
    // nothing and keeps the atlas from ballooning as the scale goes up.
    let big: String = ('A'..='Z').chain('a'..='z').chain('0'..='9').chain([' ']).collect();
    while bucket <= MAX_RASTER {
        measure_text(&big, None, bucket as u16, 1.0);
        bucket += SIZE_STEP;
    }
}

pub fn draw_centered(text: &str, y: f32, size: f32, color: Color) {
    let dims = msize(text, size);
    dtext(text, screen_width() / 2.0 - dims.width / 2.0, y, size, color);
}

/// Text centered on a column, shrunk to fit its width — the side-gutter HUD
/// uses this so nothing spills onto the highway.
pub fn draw_fit(text: &str, cx: f32, y: f32, size: f32, max_w: f32, color: Color) {
    let mut s = size;
    let d = msize(text, s);
    if d.width > max_w {
        s *= max_w / d.width;
    }
    let d = msize(text, s);
    dtext(text, cx - d.width / 2.0, y, s, color);
}

pub const FRAME_LOG_LEN: usize = 240;

/// F1 overlay: recent frame times as 1px bars against a 60 fps reference
/// line, with the worst frame in the window called out. Spikes paint red.
pub fn draw_frame_graph(log: &std::collections::VecDeque<f32>) {
    let k = ui();
    let bar = k.max(1.0); // one bar per frame, at least a pixel wide
    let (w, h) = (FRAME_LOG_LEN as f32 * bar, 64.0 * k);
    let x0 = 14.0 * k;
    let y1 = screen_height() - 14.0 * k;
    let scale = h / 0.034; // graph top ≈ 34 ms, two 60 Hz frames
    let pad = 6.0 * k;
    draw_rectangle(
        x0 - pad,
        y1 - h - pad,
        w + pad * 2.0,
        h + pad * 2.0,
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    for (i, &dt) in log.iter().enumerate() {
        let bh = (dt * scale).clamp(1.0, h);
        let c = if dt > 1.0 / 45.0 { th().miss } else { th().secondary };
        draw_rectangle(x0 + i as f32 * bar, y1 - bh, bar, bh, wa(c, 0.9));
    }
    let y60 = y1 - scale / 60.0;
    draw_line(x0, y60, x0 + w, y60, 1.0, Color::new(1.0, 1.0, 1.0, 0.4));
    let worst = log.iter().copied().fold(0.0f32, f32::max);
    dtext(
        &format!("{} fps   worst {:.1} ms", get_fps(), worst * 1000.0),
        x0,
        y1 - h - 14.0 * k,
        16.0 * k,
        Color::new(1.0, 1.0, 1.0, 0.8),
    );
}
