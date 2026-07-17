// Text and overlay drawing: the quantized pixel-font helpers, centered/
// fitted text, and the F1 frame-time graph.

use macroquad::prelude::*;

use crate::theme::{th, wa};

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

fn qsize(size: f32) -> (u16, f32) {
    // Bucket capped at 200 px: a corrupt/huge size scales a cached quad up
    // instead of rasterizing a giant glyph that would explode the atlas
    let bucket = (size / SIZE_STEP).ceil().clamp(1.0, 50.0) * SIZE_STEP;
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
pub fn prewarm_glyphs() {
    let charset: String = (' '..='~').chain(['·']).collect();
    let mut bucket = SIZE_STEP;
    while bucket <= 96.0 {
        measure_text(&charset, None, bucket as u16, 1.0);
        bucket += SIZE_STEP;
    }
    // The results-screen grade is the one glyph drawn larger
    measure_text("SABCD", None, 160, 1.0);
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
    let (w, h) = (FRAME_LOG_LEN as f32, 64.0);
    let x0 = 14.0;
    let y1 = screen_height() - 14.0;
    let scale = h / 0.034; // graph top ≈ 34 ms, two 60 Hz frames
    draw_rectangle(x0 - 6.0, y1 - h - 6.0, w + 12.0, h + 12.0, Color::new(0.0, 0.0, 0.0, 0.55));
    for (i, &dt) in log.iter().enumerate() {
        let bh = (dt * scale).clamp(1.0, h);
        let c = if dt > 1.0 / 45.0 { th().miss } else { th().secondary };
        draw_rectangle(x0 + i as f32, y1 - bh, 1.0, bh, wa(c, 0.9));
    }
    let y60 = y1 - scale / 60.0;
    draw_line(x0, y60, x0 + w, y60, 1.0, Color::new(1.0, 1.0, 1.0, 0.4));
    let worst = log.iter().copied().fold(0.0f32, f32::max);
    dtext(
        &format!("{} fps   worst {:.1} ms", get_fps(), worst * 1000.0),
        x0,
        y1 - h - 14.0,
        16.0,
        Color::new(1.0, 1.0, 1.0, 0.8),
    );
}
