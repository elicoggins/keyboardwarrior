// Keycaps, and the footer strips every screen builds out of them.
//
// One vocabulary, used on every screen: a key is always a bordered cap, the
// thing it does is always plain text beside it, and a setting's current value
// is the only accent-colored token on the line. Screens declare their footer
// as data — a slice of clusters — and the measuring, centering, dividers and
// overflow behaviour all live here, so no screen hand-packs a hint string
// again.
//
// The caps deliberately reuse the menu's keyboard-legend look (faint tinted
// fill, hairline border) so the two read as the same system. They stay
// neutral gray rather than picking up lane colors: on this screen a lane
// color means "finger zone", and spending it on chrome would dilute the one
// thing the legend is there to teach.

use macroquad::prelude::*;

use crate::gfx::{dtext, msize};
use crate::theme::{th, wa};

/// How far a strip is allowed to shrink before it starts dropping clusters
/// instead. Below roughly this the built-in pixel font stops resolving.
const MIN_SHRINK: f32 = 0.78;

/// What a cap shows.
///
/// Text only, and ASCII at that: macroquad's built-in font has no arrow or
/// symbol glyphs, and anything outside the printable set renders as a
/// missing-glyph box. Nothing here needs them — arrow-key navigation is
/// deliberately never spelled out.
#[derive(Clone, Copy)]
pub enum Cap {
    /// A literal key label: `M`, `ENTER`, `DEL`.
    Txt(&'static str),
    /// Two text caps side by side, e.g. `-` and `+`.
    Pair(&'static str, &'static str),
}

/// One `[key] label` pair, optionally carrying the value that key changes.
pub struct Item {
    cap: Cap,
    label: &'static str,
    value: String,
}

impl Item {
    /// A key and what it does. Labels are lowercase — these are verbs.
    pub fn act(cap: Cap, label: &'static str) -> Self {
        Item { cap, label, value: String::new() }
    }

    /// A key, what it changes, and what it's set to right now. Labels are
    /// uppercase — these are field names, not actions.
    pub fn stat(cap: Cap, label: &'static str, value: impl Into<String>) -> Self {
        Item { cap, label, value: value.into() }
    }
}

/// Cap and text metrics for one footer register.
///
/// Two exist. `stat` is the louder one: it reports state, so its values carry
/// the accent color and its caps sit brighter. `hint` is quieter — it lists
/// what keys do, which is reference material you read once and then stop
/// seeing, so it must never compete with the song list above it.
#[derive(Clone, Copy)]
pub struct Style {
    k: f32,
    cap_h: f32,
    glyph: f32,
    label: f32,
    label_a: f32,
    value: f32,
    fill: f32,
    line: f32,
    ink: f32,
}

impl Style {
    pub fn stat(k: f32) -> Self {
        Style {
            k,
            cap_h: 21.0 * k,
            glyph: 13.0 * k,
            label: 13.0 * k,
            label_a: 0.42,
            value: 16.0 * k,
            fill: 0.13,
            line: 0.30,
            ink: 0.82,
        }
    }

    pub fn hint(k: f32) -> Self {
        Style {
            k,
            cap_h: 19.0 * k,
            glyph: 12.0 * k,
            label: 15.0 * k,
            label_a: 0.52,
            value: 15.0 * k,
            fill: 0.06,
            line: 0.20,
            ink: 0.64,
        }
    }

    /// The same style at a fraction of the size — how a strip absorbs a
    /// window too narrow to hold it at full size.
    fn scaled(self, f: f32) -> Self {
        Style {
            k: self.k * f,
            cap_h: self.cap_h * f,
            glyph: self.glyph * f,
            label: self.label * f,
            value: self.value * f,
            ..self
        }
    }

    /// Height of the band a strip occupies, for callers stacking rows.
    pub fn height(self) -> f32 {
        self.cap_h
    }

    fn pad(self) -> f32 {
        self.cap_h * 0.38
    }

    fn cap_gap(self) -> f32 {
        self.cap_h * 0.20
    }

    fn item_gap(self) -> f32 {
        self.cap_h * 1.15
    }

    fn cluster_gap(self) -> f32 {
        self.cap_h * 2.0
    }

    /// Width of a single cap. Square for one character, growing with the
    /// label for the wider ones, so `ENTER` and `M` stay the same height.
    fn unit_w(self, txt: &str) -> f32 {
        (msize(txt, self.glyph).width + self.cap_h * 0.72).max(self.cap_h)
    }

    fn cap_w(self, cap: Cap) -> f32 {
        match cap {
            Cap::Txt(t) => self.unit_w(t),
            Cap::Pair(a, b) => self.unit_w(a) + self.cap_gap() + self.unit_w(b),
        }
    }

    fn item_w(self, it: &Item) -> f32 {
        let mut w = self.cap_w(it.cap) + self.pad() + msize(it.label, self.label).width;
        if !it.value.is_empty() {
            w += self.pad() + msize(&it.value, self.value).width;
        }
        w
    }

    fn strip_w(self, clusters: &[&[Item]]) -> f32 {
        let mut w = 0.0;
        for (ci, c) in clusters.iter().enumerate() {
            if ci > 0 {
                w += self.cluster_gap();
            }
            for (i, it) in c.iter().enumerate() {
                if i > 0 {
                    w += self.item_gap();
                }
                w += self.item_w(it);
            }
        }
        w
    }

    fn shell(self, x: f32, y: f32, w: f32) {
        draw_rectangle(x, y, w, self.cap_h, wa(WHITE, self.fill));
        draw_rectangle_lines(x, y, w, self.cap_h, (1.5 * self.k).max(1.0), wa(WHITE, self.line));
    }

    fn txt_cap(self, x: f32, y: f32, t: &str) -> f32 {
        let w = self.unit_w(t);
        self.shell(x, y, w);
        let d = msize(t, self.glyph);
        // Measured off a fixed sample rather than the cap's own text, so every
        // cap's glyph sits on one baseline no matter which letters it holds.
        let ref_h = msize("M", self.glyph).height;
        let bx = x + (w - d.width) / 2.0;
        dtext(t, bx, y + (self.cap_h + ref_h) / 2.0, self.glyph, wa(WHITE, self.ink));
        w
    }

    fn draw_cap(self, cap: Cap, x: f32, y: f32) -> f32 {
        match cap {
            Cap::Txt(t) => self.txt_cap(x, y, t),
            Cap::Pair(a, b) => {
                let lead = self.txt_cap(x, y, a) + self.cap_gap();
                lead + self.txt_cap(x + lead, y, b)
            }
        }
    }

    /// Draw one item with its caps' band vertically centered on `cy`.
    fn draw_item(&self, it: &Item, x: f32, cy: f32) -> f32 {
        let mut cur = x + self.draw_cap(it.cap, x, cy - self.cap_h / 2.0) + self.pad();
        // Label and value are different sizes, so each is centered on the cap
        // band independently rather than sharing a baseline — measured off a
        // fixed sample so the text never shifts as the value changes.
        let base = |size: f32| cy + msize("M", size).height / 2.0;
        dtext(it.label, cur, base(self.label), self.label, wa(WHITE, self.label_a));
        cur += msize(it.label, self.label).width;
        if !it.value.is_empty() {
            cur += self.pad();
            dtext(&it.value, cur, base(self.value), self.value, wa(th().accent, 0.92));
            cur += msize(&it.value, self.value).width;
        }
        cur - x
    }
}

/// Draw clusters of items as one centered strip, vertically centered on `cy`
/// and fitted into `avail` pixels.
///
/// A strip never spills off the window and never shrinks to unreadable. It
/// absorbs a narrow window by scaling down to `MIN_SHRINK`, and past that by
/// dropping whole trailing clusters — so callers must order clusters
/// most- to least-essential, and anything droppable has to be reachable
/// somewhere else (in practice, the settings screen lists everything).
pub fn draw_strip(clusters: &[&[Item]], cy: f32, avail: f32, base: Style) {
    let mut n = clusters.len();
    while n > 1 && base.scaled(MIN_SHRINK).strip_w(&clusters[..n]) > avail {
        n -= 1;
    }
    let shown = &clusters[..n];
    let w = base.strip_w(shown);
    let s = if w > avail { base.scaled((avail / w).max(MIN_SHRINK)) } else { base };

    let mut x = screen_width() / 2.0 - s.strip_w(shown) / 2.0;
    for (ci, c) in shown.iter().enumerate() {
        if ci > 0 {
            // Hairline between clusters: enough to group, not enough to
            // become a line the eye has to step over.
            let gap = s.cluster_gap();
            let arm = s.cap_h * 0.62;
            draw_line(
                x + gap / 2.0,
                cy - arm,
                x + gap / 2.0,
                cy + arm,
                (1.0 * s.k).max(1.0),
                wa(WHITE, 0.11),
            );
            x += gap;
        }
        for (i, it) in c.iter().enumerate() {
            if i > 0 {
                x += s.item_gap();
            }
            x += s.draw_item(it, x, cy);
        }
    }
}

/// A single cap drawn inline, centered on `cy` — used by the settings screen
/// to show, next to a row, the shortcut that changes it from the menu.
pub fn draw_inline_cap(cap: Cap, x: f32, cy: f32, s: Style) {
    s.draw_cap(cap, x, cy - s.cap_h / 2.0);
}

/// Full-bleed hairline rule, inset from both margins — the one divider that
/// separates a screen's body from its footer.
pub fn draw_rule(y: f32, inset: f32, k: f32) {
    draw_line(inset, y, screen_width() - inset, y, (1.0 * k).max(1.0), wa(WHITE, 0.07));
}
