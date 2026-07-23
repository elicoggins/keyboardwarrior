// Practice mode's section picker — the Clone Hero flow: a list of the
// chart's sections where ENTER marks where practice starts, a second ENTER
// marks where it ends (the same row twice loops just that section), and the
// chosen span loops in play. One panel, two homes: the menu's SHIFT+ENTER
// screen and the in-practice pause overlay both drive this.

use macroquad::prelude::*;

use crate::chart::Section;
use crate::controls::{draw_strip, Cap, Item, Style};
use crate::gfx::{dtext, msize, ui};
use crate::theme::{th, wa};

/// What a frame of picker input asks the caller to do.
pub enum PickAction {
    None,
    /// The start section was just marked (feedback tick, nothing else).
    Anchored,
    /// Back out of the picker entirely.
    Cancel,
    /// Launch practice over this inclusive section range.
    Confirm(usize, usize),
}

pub struct SectionPicker {
    pub sections: Vec<Section>,
    cursor: usize,
    anchor: Option<usize>, // the start section, once ENTER has marked it
    scroll: usize,         // first visible row — kept around the cursor in draw
}

impl SectionPicker {
    pub fn new(sections: Vec<Section>, cursor: usize) -> Self {
        let cursor = cursor.min(sections.len().saturating_sub(1));
        SectionPicker { sections, cursor, anchor: None, scroll: cursor }
    }

    /// One frame of input. `nav` is the caller's held-repeat Up/Down result,
    /// so list feel matches every other list in the game. ESC steps back out
    /// of a marked start before it cancels the picker.
    pub fn update(&mut self, nav: Option<KeyCode>, enter: bool, escape: bool) -> PickAction {
        if self.sections.is_empty() {
            return if escape || enter { PickAction::Cancel } else { PickAction::None };
        }
        match nav {
            Some(KeyCode::Up) if self.cursor > 0 => self.cursor -= 1,
            Some(KeyCode::Down) if self.cursor + 1 < self.sections.len() => self.cursor += 1,
            _ => {}
        }
        if enter {
            return match self.anchor {
                None => {
                    self.anchor = Some(self.cursor);
                    PickAction::Anchored
                }
                Some(a) => PickAction::Confirm(a.min(self.cursor), a.max(self.cursor)),
            };
        }
        if escape {
            return match self.anchor.take() {
                Some(_) => PickAction::None,
                None => PickAction::Cancel,
            };
        }
        PickAction::None
    }

    /// The centered panel, drawn over whatever the caller has on screen (the
    /// menu backdrop or the dimmed, paused board) — near-opaque so it reads
    /// on both.
    pub fn draw(&mut self) {
        let k = ui();
        let (sw, sh) = (screen_width(), screen_height());
        let pw = (sw * 0.6).clamp(320.0 * k, 620.0 * k);
        let ph = (sh * 0.68).min(600.0 * k);
        let px = (sw - pw) / 2.0;
        let py = (sh - ph) / 2.0;
        draw_rectangle(px, py, pw, ph, wa(th().bg, 0.96));
        draw_rectangle(px, py, pw, ph, Color::new(1.0, 1.0, 1.0, 0.05));
        draw_rectangle_lines(px, py, pw, ph, 2.0 * k, wa(th().accent, 0.6));

        let title = "SELECT SECTIONS";
        let td = msize(title, 26.0 * k);
        dtext(title, px + pw / 2.0 - td.width / 2.0, py + 42.0 * k, 26.0 * k, wa(WHITE, 0.95));
        let hint = match self.anchor {
            None => "choose where practice STARTS",
            Some(_) => "choose where it ENDS - the same row loops one section",
        };
        let hd = msize(hint, 16.0 * k);
        dtext(
            hint,
            px + pw / 2.0 - hd.width / 2.0,
            py + 70.0 * k,
            16.0 * k,
            wa(th().secondary, 0.75),
        );

        // Rows, windowed around the cursor
        let list_top = py + 94.0 * k;
        let row_h = 34.0 * k;
        let list_bot = py + ph - 60.0 * k;
        let visible = (((list_bot - list_top) / row_h).floor() as usize).max(1);
        self.scroll = self.scroll.min(self.cursor);
        if self.cursor >= self.scroll + visible {
            self.scroll = self.cursor + 1 - visible;
        }
        self.scroll = self.scroll.min(self.sections.len().saturating_sub(visible));

        let (lo, hi) = match self.anchor {
            Some(a) => (a.min(self.cursor), a.max(self.cursor)),
            None => (self.cursor, self.cursor),
        };
        let name_size = 18.0 * k;
        let time_size = 15.0 * k;
        for (i, s) in self.sections.iter().enumerate().skip(self.scroll).take(visible) {
            let y = list_top + (i - self.scroll) as f32 * row_h;
            let in_range = i >= lo && i <= hi;
            if in_range {
                let a = if i == self.cursor { 0.16 } else { 0.08 };
                draw_rectangle(
                    px + 14.0 * k,
                    y,
                    pw - 28.0 * k,
                    row_h - 4.0 * k,
                    wa(th().accent, a),
                );
            }
            let base_y = y + (row_h - 4.0 * k) / 2.0 + msize("M", name_size).height / 2.0;
            if i == self.cursor {
                dtext(">", px + 16.0 * k, base_y, name_size, wa(WHITE, 0.8));
            }
            let time = fmt_time(s.start);
            let time_w = msize(&time, time_size).width;
            // The marked start keeps its accent while the end is chosen, so
            // the pending range always shows which edge is anchored
            let color = if self.anchor == Some(i) {
                wa(th().accent, 0.95)
            } else if i == self.cursor {
                wa(WHITE, 0.95)
            } else if in_range {
                wa(WHITE, 0.8)
            } else {
                wa(WHITE, 0.55)
            };
            let name_max = pw - 28.0 * k - 22.0 * k - time_w - 30.0 * k;
            let name = fit_text(&s.name, name_size, name_max);
            dtext(&name, px + 36.0 * k, base_y, name_size, color);
            dtext(&time, px + pw - 24.0 * k - time_w, base_y, time_size, wa(th().secondary, 0.55));
        }

        // Slim track down the right edge once the list overflows
        if self.sections.len() > visible {
            let (tx, ty) = (px + pw - 8.0 * k, list_top);
            let track_h = visible as f32 * row_h - 4.0 * k;
            let frac = visible as f32 / self.sections.len() as f32;
            let thumb_h = (track_h * frac).max(14.0 * k);
            let denom = (self.sections.len() - visible).max(1) as f32;
            let pos = self.scroll as f32 / denom;
            draw_rectangle(tx, ty, 3.0 * k, track_h, Color::new(1.0, 1.0, 1.0, 0.10));
            draw_rectangle(
                tx,
                ty + (track_h - thumb_h) * pos,
                3.0 * k,
                thumb_h,
                wa(th().secondary, 0.7),
            );
        }

        let s = Style::hint(k);
        let items = [
            Item::act(
                Cap::Txt("ENTER"),
                if self.anchor.is_none() { "start here" } else { "end here" },
            ),
            Item::act(Cap::Txt("ESC"), "back"),
        ];
        draw_strip(&[&items], py + ph - 30.0 * k, pw - 40.0 * k, s);
    }
}

/// mm:ss for a song position.
fn fmt_time(t: f64) -> String {
    let t = t.max(0.0) as i64;
    format!("{}:{:02}", t / 60, t % 60)
}

/// Truncate to fit `max_w` pixels, with an ellipsis when anything was cut.
fn fit_text(text: &str, size: f32, max_w: f32) -> String {
    if msize(text, size).width <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while !out.is_empty() && msize(&format!("{out}..."), size).width > max_w {
        out.pop();
    }
    format!("{out}...")
}
