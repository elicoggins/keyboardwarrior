// Keyboard Warrior — a rhythm typing game.
// Guitar-hero note highway where every gem is a letter; lanes map to finger
// zones on a QWERTY keyboard. Plays real Clone Hero songs (.sng or folders)
// through its own cpal mixer, whose sample counter IS the game clock.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

use macroquad::prelude::*;

mod audio;
mod chart;
#[cfg(not(target_arch = "wasm32"))]
mod chorus;
#[cfg(not(target_arch = "wasm32"))]
mod config;
mod controls;
mod decode;
mod gfx;
mod play;
mod settings;
mod sng;
mod theme;
#[cfg(target_arch = "wasm32")]
mod web;
mod words;

use audio::{make_sounds, AudioEngine, Buf};
use chart::{Instrument, SongChart, SongSource, DIFF_NAMES};
use controls::{draw_inline_cap, draw_rule, draw_strip, Cap, Item, Style};
use gfx::{draw_centered, draw_frame_graph, dtext, msize, prewarm_glyphs, ui, FRAME_LOG_LEN};
use play::{is_typeable, lane_of, Judgement, Play, Results, SongRef};
use settings::{
    calib_offset, cycle, settings_lines, settings_rows, SettingLine, SettingRow, CALIB_MS,
    CALIB_PERIOD, SPEEDS, SPEED_IDX,
};
use theme::{th, wa, THEMES, THEME_IDX};
use words::{TEXT_MODES, TEXT_MODE_IDX};

/// Side margin every footer strip and rule is inset by, so nothing on a
/// screen's bottom edge ever runs into the window frame.
const FOOTER_INSET: f32 = 40.0;

/// Total height the menu's footer block claims — legend, scoring line, rule,
/// status strip, hint strip and the bottom margin. The song wheel treats this
/// as its floor, so the two can never overlap however short the window gets.
const FOOTER_H: f32 = 258.0;

/// Seconds a corner toast stays up before it has fully faded away.
const TOAST_LIFE: f64 = 3.2;

/// A transient status message ("added …", "deleted …", a load failure) shown
/// briefly in the menu's corner instead of parked across the screen.
struct Toast {
    text: String,
    born: f64,
}

impl Toast {
    fn new(text: impl Into<String>) -> Self {
        Toast { text: text.into(), born: get_time() }
    }
}

/// Mini keyboard legend drawn in the menu: every key tinted by its lane, so
/// the lane-to-hand mapping is shown, not spelled out.
fn draw_keyboard_legend(center_x: f32, top_y: f32) {
    let rows: [(&str, f32); 3] = [("qwertyuiop", 0.0), ("asdfghjkl;", 0.4), ("zxcvbnm,.", 1.0)];
    let k = ui();
    let key = 26.0 * k;
    let gap = 5.0 * k;
    let full = 10.0 * (key + gap) - gap;
    for (ri, (row, stagger)) in rows.iter().enumerate() {
        let y = top_y + ri as f32 * (key + gap);
        let x0 = center_x - full / 2.0 + stagger * (key + gap) * 0.5;
        for (ci, ch) in row.chars().enumerate() {
            let x = x0 + ci as f32 * (key + gap);
            let c = th().lane[lane_of(ch)];
            draw_rectangle(x, y, key, key, wa(c, 0.14));
            draw_rectangle_lines(x, y, key, key, 1.5 * k, wa(c, 0.45));
            let label = ch.to_ascii_uppercase().to_string();
            let size = 13.0 * k;
            let d = msize(&label, size);
            dtext(
                &label,
                x + key / 2.0 - d.width / 2.0,
                y + key / 2.0 + d.height / 2.0,
                size,
                wa(c, 0.85),
            );
        }
    }
}

/// Latency calibration: tap along to a metronome, apply the median offset.
struct Calibrate {
    taps: Vec<f64>,       // signed tap offsets vs the nearest tick, seconds
    scheduled_until: f64, // timeline time up to which ticks are queued
    menu_sel: usize,      // menu selection to restore on the way back out
    from_menu: bool,      // entered via the menu hotkey, not the settings row
    off_ms: i64,          // working offset: tap median or hand-nudged, applied on ENTER
}

enum Scene {
    Menu {
        sel: usize,
        diff_sel: usize,
        scroll: f32,
        // When Some, Enter has been pressed on a difficulty that charts two
        // comparable instruments, so the difficulty row is showing a
        // guitar/bass chooser instead: (options, highlighted index). Moving up
        // or down the song list cancels it, back to difficulty selection.
        pick: Option<(Vec<Instrument>, usize)>,
    },
    Settings {
        sel: usize,
        menu_sel: usize,
    },
    Loading {
        rx: Receiver<LoadMsg>,
        song: usize,
        diff: usize,
        title: String,
    },
    Playing(Box<Play>),
    Results(Results),
    Calibrate(Calibrate),
    // In-app Chorus Encore browser: type a query, download a chart into songs/.
    #[cfg(not(target_arch = "wasm32"))]
    Chorus(Box<ChorusScene>),
}

/// Where the Chorus results list sits and how many rows fit on screen.
/// Scrolling and drawing both derive from this, so the row the selection
/// thinks is visible is always the row that actually gets drawn.
#[cfg(not(target_arch = "wasm32"))]
fn chorus_list_geom(k: f32) -> (f32, f32, usize) {
    let list_top = (192.0 + 128.0) * k; // search box top + list offset
    let row_h = 52.0 * k;
    let avail = (screen_height() - 90.0 * k) - list_top;
    // +1 because a row sitting exactly on the last pixel still draws
    let visible = ((avail / row_h).floor() as i32 + 1).max(1) as usize;
    (list_top, row_h, visible)
}

/// The next selectable song when moving through the (already filtered) menu
/// `view`. Steps one row in `dir` (Up/anything-else = Down), wrapping, and
/// skips locked signpost rows. `sel` and the result are full-list indices;
/// returns None only when the view is empty.
fn step_selection(
    view: &[usize],
    songs: &[chart::SongEntry],
    sel: usize,
    dir: KeyCode,
) -> Option<usize> {
    if view.is_empty() {
        return None;
    }
    let n = view.len();
    let cur = view.iter().position(|&i| i == sel).unwrap_or(0);
    let mut p = cur;
    for _ in 0..n {
        p = match dir {
            KeyCode::Up => (p + n - 1) % n,
            _ => (p + 1) % n,
        };
        if !songs[view[p]].locked {
            return Some(view[p]);
        }
    }
    // Every visible row is a locked signpost — stay where we are.
    Some(view[cur])
}

/// The menu's difficulty-selector position for a song's played difficulty.
/// Returning from a run lands on the difficulty just played — where its best
/// score is shown — instead of snapping back to the easiest.
fn diff_pos(songs: &[chart::SongEntry], song: usize, diff: usize) -> usize {
    songs.get(song).and_then(|s| s.available.iter().position(|&d| d == diff)).unwrap_or(0)
}

/// Whether keystrokes edit the query or navigate the results list.
#[cfg(not(target_arch = "wasm32"))]
#[derive(PartialEq, Clone, Copy)]
enum ChorusFocus {
    Search,  // typing edits the query; Enter runs a search
    Results, // up/down move the selection; Enter downloads
}

/// State for the Chorus search screen. Network calls run on worker threads and
/// report back over `net`, so the render loop never blocks.
#[cfg(not(target_arch = "wasm32"))]
struct ChorusScene {
    query: String,          // the text being typed
    focus: ChorusFocus,     // query bar vs results list
    diff_idx: usize,        // index into chorus::DIFF_FILTERS
    hits: Vec<chorus::Hit>, // results of the last search
    sel: usize,             // highlighted result
    scroll: usize,          // first result row drawn — the list pans
    // with the selection instead of letting
    // it walk off the bottom of the screen
    menu_sel: usize,                  // menu row to restore on Escape
    net: Option<Receiver<ChorusMsg>>, // in-flight search/download, if any
    busy: &'static str,               // "" idle, else "searching…" / "downloading…"
    note: Option<String>,             // last error/status shown on the screen
}

#[cfg(not(target_arch = "wasm32"))]
enum ChorusMsg {
    Results(Result<Vec<chorus::Hit>, String>),
    Downloaded(Result<String, String>), // Ok(title) once saved into songs/
}

// The cached mix is instrument-specific: guitar and bass duck different stems,
// so the key carries which instrument produced this backing+lead.
type StemCache = Option<(SongSource, Instrument, Buf, Option<Buf>)>;

struct LoadedSong {
    chart: SongChart,
    backing: Buf,
    lead: Option<Buf>,
}

enum LoadMsg {
    Done(Box<LoadedSong>),
    Failed(String),
}

/// Kick off a song load on a worker thread so the render loop keeps
/// animating; the Loading scene polls the returned channel.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_loader(
    source: SongSource,
    instrument: Instrument,
    rate: u32,
    cached: StemCache,
) -> Receiver<LoadMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let msg = match load_song_full(&source, instrument, rate, cached) {
            Ok(l) => LoadMsg::Done(Box::new(l)),
            Err(e) => LoadMsg::Failed(e),
        };
        let _ = tx.send(msg);
    });
    rx
}

/// Reveal a folder in the OS file manager (Finder / Explorer / the default
/// file browser) so the player can drag a downloaded .sng straight in.
#[cfg(not(target_arch = "wasm32"))]
fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    // Make sure the target exists so the file manager doesn't error out.
    let _ = std::fs::create_dir_all(path);
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(path).spawn().map(|_| ())
}

/// No threads on wasm: the decode is deferred a couple of frames (so the
/// loading screen is visible first) and then runs synchronously on the game
/// thread — see web::pump in the main loop.
#[cfg(target_arch = "wasm32")]
fn spawn_loader(
    source: SongSource,
    instrument: Instrument,
    rate: u32,
    cached: StemCache,
) -> Receiver<LoadMsg> {
    let (tx, rx) = channel();
    web::defer(move || {
        let msg = match load_song_full(&source, instrument, rate, cached) {
            Ok(l) => LoadMsg::Done(Box::new(l)),
            Err(e) => LoadMsg::Failed(e),
        };
        let _ = tx.send(msg);
    });
    rx
}

/// Parse the chart and decode stems straight from their source (folder or
/// .sng — no conversions). Stems are decoded one at a time and summed into
/// the backing mix as they finish, so peak memory is the mix plus a single
/// stem — not every decoded stem at once.
fn load_song_full(
    source: &SongSource,
    instrument: Instrument,
    rate: u32,
    cached: StemCache,
) -> Result<LoadedSong, String> {
    let charts = chart::load_song(source)?;
    let chart = chart::pick_chart(charts, instrument).ok_or("song has no playable chart")?;
    if let Some((src, inst, backing, lead)) = cached {
        if src == *source && inst == instrument {
            return Ok(LoadedSong { chart, backing, lead });
        }
    }
    let stems = chart::stem_files(source)?;
    if stems.is_empty() {
        return Err("no audio stems found".into());
    }
    let lead_names = chart::lead_stem_names(instrument);
    let mut mix: Vec<[f32; 2]> = Vec::new();
    let mut lead: Option<Buf> = None;
    let mut failures: Vec<String> = Vec::new();
    for (name, bytes) in stems {
        match decode::decode(&bytes, &name, rate) {
            Ok(buf) => {
                let base = name.rsplit_once('.').map(|(b, _)| b.to_lowercase()).unwrap_or_default();
                if lead.is_none() && lead_names.contains(&base.as_str()) {
                    lead = Some(buf);
                } else {
                    decode::mix_into(&mut mix, &buf);
                }
            }
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    // Single-stream songs: the whole mix is the backing, no ducking
    if mix.is_empty() {
        match lead.take() {
            Some(l) => mix = Arc::try_unwrap(l).unwrap_or_else(|a| (*a).clone()),
            None => {
                return Err(if failures.is_empty() {
                    "no audio stems decoded".to_string()
                } else {
                    failures.join("  ·  ")
                });
            }
        }
    }
    let (backing, lead) = decode::finalize_mix(mix, lead);
    Ok(LoadedSong { chart, backing, lead })
}

fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Hold time before a held navigation key starts repeating on its own
const REPEAT_DELAY: f32 = 0.32;
/// Gap before the first auto-repeat tick
const REPEAT_SLOW: f32 = 0.13;
/// Floor the gap accelerates down to
const REPEAT_FAST: f32 = 0.08;
/// Each tick waits this fraction of the previous gap
const REPEAT_ACCEL: f32 = 0.82;

/// Auto-repeat for held navigation keys. A tap fires once; holding the key
/// past `REPEAT_DELAY` starts it ticking, and each tick lands a little sooner
/// than the last until it tops out at `REPEAT_FAST`.
struct KeyRepeat {
    /// The key currently owning the repeat — the last one pressed, so
    /// tapping the opposite direction mid-hold takes over immediately
    key: Option<KeyCode>,
    /// Seconds until the next tick
    timer: f32,
    /// Gap the next tick will use
    interval: f32,
}

impl KeyRepeat {
    fn new() -> Self {
        Self { key: None, timer: 0.0, interval: REPEAT_SLOW }
    }

    /// Advances the repeat clock one frame and returns the key that should
    /// act this frame, if any. Call once per frame with the keys that share
    /// the repeat (e.g. Up and Down of one list).
    fn poll(&mut self, keys: &[KeyCode], dt: f32) -> Option<KeyCode> {
        // A fresh press always wins and restarts the ramp from the top
        if let Some(&k) = keys.iter().find(|&&k| is_key_pressed(k)) {
            self.key = Some(k);
            self.timer = REPEAT_DELAY;
            self.interval = REPEAT_SLOW;
            return Some(k);
        }
        let held = self.key.filter(|&k| keys.contains(&k) && is_key_down(k))?;
        self.timer -= dt;
        if self.timer > 0.0 {
            return None;
        }
        self.timer = self.interval;
        self.interval = (self.interval * REPEAT_ACCEL).max(REPEAT_FAST);
        Some(held)
    }
}

/// Procedural app icon in the EMBER theme: dark rounded square, pale strike
/// line, and an amber-ringed gem sitting on it — the game in one glyph.
fn icon_pixels<const N: usize>(size: usize) -> [u8; N] {
    let s = size as f32;
    let mut px = vec![0u8; size * size * 4];
    let put = |px: &mut Vec<u8>, x: usize, y: usize, c: [f32; 4]| {
        let i = (y * size + x) * 4;
        px[i] = (c[0] * 255.0) as u8;
        px[i + 1] = (c[1] * 255.0) as u8;
        px[i + 2] = (c[2] * 255.0) as u8;
        px[i + 3] = (c[3] * 255.0) as u8;
    };
    let blend = |base: [f32; 4], top: [f32; 3], a: f32| {
        [
            base[0] + (top[0] - base[0]) * a,
            base[1] + (top[1] - base[1]) * a,
            base[2] + (top[2] - base[2]) * a,
            base[3].max(a),
        ]
    };
    let bg = [0.055f32, 0.057, 0.066];
    let amber = [0.96f32, 0.62, 0.12];
    let pale = [0.85f32, 0.88, 0.92];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square silhouette
            let r = s * 0.19;
            let (cx, cy) = (fx.clamp(r, s - r), fy.clamp(r, s - r));
            let corner = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let mask = (1.0 - (corner - r + 0.5)).clamp(0.0, 1.0);
            if mask <= 0.0 {
                put(&mut px, x, y, [0.0, 0.0, 0.0, 0.0]);
                continue;
            }
            let mut c = [bg[0], bg[1], bg[2], mask];
            // Strike line at the lower third
            let line_y = s * 0.74;
            let line_a = (1.0 - ((fy - line_y).abs() - s * 0.02).max(0.0) * 2.0).clamp(0.0, 1.0);
            c = blend(c, pale, line_a * 0.75);
            // Gem: soft glow, dark body, thick amber ring
            let d = ((fx - s * 0.5).powi(2) + (fy - line_y + s * 0.24).powi(2)).sqrt();
            let ring_r = s * 0.22;
            let glow = (1.0 - ((d - ring_r) / (s * 0.14)).max(0.0)).clamp(0.0, 1.0);
            c = blend(c, amber, glow * glow * 0.25);
            if d < ring_r {
                let body = blend(c, amber, 0.16);
                c = [body[0], body[1], body[2], c[3]];
            }
            let ring_a = (1.0 - ((d - ring_r).abs() - s * 0.045).max(0.0) * (3.0 / (s / 16.0)))
                .clamp(0.0, 1.0);
            c = blend(c, amber, ring_a);
            c[3] *= mask;
            put(&mut px, x, y, c);
        }
    }
    px.try_into().unwrap_or([0; N])
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Keyboard Warrior".to_string(),
        window_width: 1100,
        window_height: 800,
        high_dpi: true,
        icon: Some(miniquad::conf::Icon {
            small: icon_pixels::<{ 16 * 16 * 4 }>(16),
            medium: icon_pixels::<{ 32 * 32 * 4 }>(32),
            big: icon_pixels::<{ 64 * 64 * 4 }>(64),
        }),
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    macroquad::rand::srand(macroquad::miniquad::date::now() as u64);
    prewarm_glyphs();
    let engine = AudioEngine::new();
    let sounds = make_sounds(engine.sample_rate);
    // The bundled songs/ dir is always scanned first; the player's extra
    // folders (from the config file / KW_SONG_DIRS) are added on top.
    #[cfg(not(target_arch = "wasm32"))]
    let mut config = config::Config::load();
    // Restore persisted settings (theme, speed, text mode, calibration,
    // volume) before the first frame paints, then keep a snapshot so any later
    // change is written straight back out.
    #[cfg(not(target_arch = "wasm32"))]
    config::load_settings(&engine);
    #[cfg(not(target_arch = "wasm32"))]
    let mut last_settings = config::settings_snapshot(&engine);
    // Persisted personal-best scores, surfaced on the menu and results screens.
    #[cfg(not(target_arch = "wasm32"))]
    let mut scores = config::Scores::load();
    #[cfg(not(target_arch = "wasm32"))]
    let song_roots = |cfg: &config::Config| -> Vec<std::path::PathBuf> {
        let mut roots = vec![std::path::PathBuf::from("songs")];
        roots.extend(cfg.song_dirs.iter().cloned());
        roots
    };
    #[cfg(not(target_arch = "wasm32"))]
    let mut songs = chart::scan_all(&song_roots(&config));
    #[cfg(target_arch = "wasm32")]
    let songs = web::load_demo_library().await;
    let mut stem_cache: StemCache = None;
    // Transient status message shown briefly in the menu corner, then it fades
    let mut toast: Option<Toast> = None;
    // Menu search: `;` opens a filter bar over the song list. Some(query) means
    // the bar is open and typing edits the query (a case-insensitive match over
    // title + artist); None means the normal single-key hotkeys are live.
    let mut menu_search: Option<String> = None;
    // Song index armed for deletion: Delete once arms the selected row, Delete
    // again on the same row removes it. Any navigation disarms.
    #[cfg(not(target_arch = "wasm32"))]
    let mut pending_delete: Option<usize> = None;
    let mut scene = Scene::Menu { sel: 0, diff_sel: 0, scroll: 0.0, pick: None };

    // Debug hook: KW_AUTOSTART=<song>:<diff> jumps straight into a song
    if let Ok(v) = std::env::var("KW_AUTOSTART") {
        let mut it = v.split(':');
        let s: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let d: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        if s < songs.len() {
            let instrument =
                songs[s].charts.first().map(|c| c.instrument).unwrap_or(Instrument::Guitar);
            let rx = spawn_loader(songs[s].source.clone(), instrument, engine.sample_rate, None);
            scene = Scene::Loading { rx, song: s, diff: d, title: songs[s].title.clone() };
        }
    }

    // Frame-time overlay (F1), for chasing stutter by eye
    let mut show_frame_graph = false;
    let mut frame_log: std::collections::VecDeque<f32> =
        std::collections::VecDeque::with_capacity(FRAME_LOG_LEN);
    // Seconds left on the master-volume overlay after a -/+ press
    let mut vol_flash = 0.0f32;
    // Held Up/Down repeats when scrolling any of the vertical lists
    let mut nav_repeat = KeyRepeat::new();

    loop {
        // Deferred decode jobs (wasm has no loader threads) run here, after
        // their loading screen has had two frames to reach the display
        #[cfg(target_arch = "wasm32")]
        web::pump();
        // Buffers the audio callback retired get freed here, off the
        // real-time thread
        engine.reap();
        if is_key_pressed(KeyCode::F1) {
            show_frame_graph = !show_frame_graph;
        }
        // Master volume: -/+ adjusts it from any scene, with a tick at the
        // new level and a brief overlay to confirm
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::Equal) {
            let step = if is_key_pressed(KeyCode::Minus) { -0.05f32 } else { 0.05 };
            engine.set_master(((engine.master() + step) * 20.0).round() / 20.0);
            engine.play(&sounds.hat, 0.6);
            vol_flash = 1.6;
        }
        if frame_log.len() == FRAME_LOG_LEN {
            frame_log.pop_front();
        }
        frame_log.push_back(get_frame_time());
        match &mut scene {
            Scene::Menu { sel, diff_sel, scroll, pick } => {
                let rows = songs.len();
                if rows == 0 {
                    let k = ui();
                    clear_background(th().bg);
                    draw_centered(
                        "KEYBOARD WARRIOR",
                        130.0 * k,
                        72.0 * k,
                        Color::new(1.0, 1.0, 1.0, 0.95),
                    );
                    draw_centered(
                        if cfg!(target_arch = "wasm32") {
                            "demo song unavailable - refresh the page to retry"
                        } else {
                            "no songs found - drop a Clone Hero .sng or song folder into songs/"
                        },
                        screen_height() * 0.5,
                        22.0 * k,
                        wa(th().secondary, 0.8),
                    );
                    next_frame().await;
                    continue;
                }
                // Search bar: `;` opens a filter over the list. While it's open,
                // typing edits the query and the letter hotkeys below are
                // suspended so they don't fire as text; Escape closes and clears
                // it, Backspace erases, every other printable key types.
                if let Some(q) = menu_search.as_mut() {
                    // Only printable ASCII reaches the query — the font atlas
                    // holds ' '..='~' only, so arrow keys and other special keys
                    // (delivered by get_char_pressed as non-printable chars that
                    // slip past is_control) would otherwise draw as tofu.
                    while let Some(c) = get_char_pressed() {
                        if (' '..='~').contains(&c) {
                            q.push(c);
                        }
                    }
                    if is_key_pressed(KeyCode::Backspace) {
                        q.pop();
                    }
                }
                // Escape closes the bar, `;` opens it — both reassign
                // `menu_search`, so they sit outside the `as_mut()` borrow above.
                if menu_search.is_some() {
                    if is_key_pressed(KeyCode::Escape) {
                        menu_search = None;
                    }
                } else if is_key_pressed(KeyCode::Escape) && pick.is_some() {
                    // Back out of the guitar/bass chooser to difficulty selection.
                    *pick = None;
                    engine.play(&sounds.hat, 0.4);
                } else if is_key_pressed(KeyCode::Semicolon) {
                    menu_search = Some(String::new());
                    // Swallow the ';' so it doesn't land in the fresh query.
                    while get_char_pressed().is_some() {}
                    engine.play(&sounds.hat, 0.4);
                }
                let searching = menu_search.is_some();

                // Song rows visible under the current filter, in list order. An
                // empty/whitespace query (or a closed bar) shows everything.
                let query = menu_search.as_deref().unwrap_or("").trim().to_lowercase();
                let view: Vec<usize> = if query.is_empty() {
                    (0..rows).collect()
                } else {
                    (0..rows)
                        .filter(|&i| {
                            songs[i].title.to_lowercase().contains(&query)
                                || songs[i].artist.to_lowercase().contains(&query)
                        })
                        .collect()
                };
                // Keep the selection on a visible row: if the filter just hid
                // it, jump to the first selectable match (skipping locked
                // signpost rows) and reset the difficulty pick.
                if !view.is_empty() && !view.contains(&*sel) {
                    *sel = view.iter().copied().find(|&i| !songs[i].locked).unwrap_or(view[0]);
                    *diff_sel = 0;
                    *pick = None;
                }

                // Difficulty options for the selected song. A broken song has
                // none — saturating_sub keeps diff_sel at 0 rather than underflow.
                let diff_opts: Vec<(usize, String)> =
                    songs[*sel].available.iter().map(|&d| (d, DIFF_NAMES[d].to_string())).collect();
                *diff_sel = (*diff_sel).min(diff_opts.len().saturating_sub(1));

                let nav = nav_repeat.poll(&[KeyCode::Up, KeyCode::Down], get_frame_time());
                if let Some(dir) = nav {
                    if let Some(next) = step_selection(&view, &songs, *sel, dir) {
                        *sel = next;
                        *diff_sel = 0;
                        // Moving to another song leaves the chooser behind — the
                        // pick isn't kept for when you come back.
                        *pick = None;
                        engine.play(&sounds.hat, 0.4);
                    }
                }
                // Moving off a row cancels a pending deletion on it.
                #[cfg(not(target_arch = "wasm32"))]
                if nav.is_some() && pending_delete.is_some_and(|d| d != *sel) {
                    pending_delete = None;
                }
                // Left/right pick the instrument while the chooser is up, else
                // they cycle the difficulty.
                if let Some((options, isel)) = pick.as_mut() {
                    if is_key_pressed(KeyCode::Left) && *isel > 0 {
                        *isel -= 1;
                        engine.play(&sounds.hat, 0.4);
                    }
                    if is_key_pressed(KeyCode::Right) && *isel + 1 < options.len() {
                        *isel += 1;
                        engine.play(&sounds.hat, 0.4);
                    }
                } else {
                    if is_key_pressed(KeyCode::Left) && *diff_sel > 0 {
                        *diff_sel -= 1;
                        engine.play(&sounds.hat, 0.4);
                    }
                    if is_key_pressed(KeyCode::Right) && *diff_sel + 1 < diff_opts.len() {
                        *diff_sel += 1;
                        engine.play(&sounds.hat, 0.4);
                    }
                }
                // Hotkeys for the common settings, mirrored on the settings
                // screen (O) — regulars shouldn't need to leave the menu. All
                // of them are suspended while the search bar is open so their
                // letters type into the query instead.
                if !searching && is_key_pressed(KeyCode::M) {
                    cycle(&TEXT_MODE_IDX, TEXT_MODES.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if !searching && is_key_pressed(KeyCode::T) {
                    cycle(&THEME_IDX, THEMES.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if !searching && is_key_pressed(KeyCode::V) {
                    cycle(&SPEED_IDX, SPEEDS.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if !searching && is_key_pressed(KeyCode::C) {
                    engine.play(&sounds.kick, 0.4);
                    engine.start_timeline(1.0, None, None);
                    scene = Scene::Calibrate(Calibrate {
                        taps: Vec::new(),
                        scheduled_until: 0.0,
                        menu_sel: *sel,
                        from_menu: true,
                        off_ms: CALIB_MS.load(Ordering::Relaxed),
                    });
                    next_frame().await;
                    continue;
                }
                if !searching && is_key_pressed(KeyCode::S) {
                    engine.play(&sounds.kick, 0.4);
                    scene = Scene::Settings { sel: 0, menu_sel: *sel };
                    next_frame().await;
                    continue;
                }
                // Song-library management (native only — the browser demo has a
                // fixed library and no filesystem).
                #[cfg(not(target_arch = "wasm32"))]
                if !searching && is_key_pressed(KeyCode::A) {
                    engine.play(&sounds.kick, 0.4);
                    // The picker is modal and blocks the render loop; that's
                    // fine — the player is deliberately paused on a dialog.
                    if let Some(dir) =
                        rfd::FileDialog::new().set_title("Add a song folder").pick_folder()
                    {
                        let shown = dir.display().to_string();
                        if config.add_song_dir(dir) {
                            songs = chart::scan_all(&song_roots(&config));
                            *sel = 0;
                            *scroll = 0.0;
                            toast = Some(Toast::new(format!(
                                "added {shown} - {} songs total",
                                songs.len()
                            )));
                        } else {
                            toast = Some(Toast::new(format!("{shown} is already in your library")));
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if !searching && is_key_pressed(KeyCode::F) {
                    engine.play(&sounds.kick, 0.4);
                    if let Err(e) = open_in_file_manager(std::path::Path::new("songs")) {
                        toast = Some(Toast::new(format!("couldn't open songs folder: {e}")));
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if !searching && is_key_pressed(KeyCode::R) {
                    engine.play(&sounds.kick, 0.4);
                    songs = chart::scan_all(&song_roots(&config));
                    *sel = (*sel).min(songs.len().saturating_sub(1));
                    pending_delete = None;
                    toast = Some(Toast::new(format!("rescanned - {} songs", songs.len())));
                }
                // Delete: two presses on the same row remove the song from disk.
                // (Suspended while searching, where Backspace erases the query.)
                #[cfg(not(target_arch = "wasm32"))]
                if !searching
                    && (is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace))
                {
                    let row = *sel;
                    if chart::is_bundled(&songs[row].source) {
                        toast = Some(Toast::new("bundled default songs can't be deleted"));
                        pending_delete = None;
                    } else if pending_delete == Some(row) {
                        // Confirmed — remove it, then rescan and fix selection.
                        let title = songs[row].title.clone();
                        match chart::delete_song(&songs[row].source) {
                            Ok(()) => {
                                songs = chart::scan_all(&song_roots(&config));
                                *sel = row.min(songs.len().saturating_sub(1));
                                toast = Some(Toast::new(format!("deleted {title}")));
                                engine.play(&sounds.kick, 0.5);
                                // Restart the frame: `view`/`diff_opts` above were
                                // built off the old, longer list and would index
                                // out of bounds against the freshly shrunk `songs`.
                                pending_delete = None;
                                next_frame().await;
                                continue;
                            }
                            Err(e) => {
                                toast = Some(Toast::new(format!("couldn't delete {title}: {e}")))
                            }
                        }
                        pending_delete = None;
                    } else {
                        // Arm it: the row now shows a confirm prompt.
                        pending_delete = Some(row);
                        engine.play(&sounds.hat, 0.4);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if !searching && is_key_pressed(KeyCode::G) {
                    engine.play(&sounds.kick, 0.4);
                    scene = Scene::Chorus(Box::new(ChorusScene {
                        query: String::new(),
                        focus: ChorusFocus::Search,
                        diff_idx: 0,
                        hits: Vec::new(),
                        sel: 0,
                        scroll: 0,
                        menu_sel: *sel,
                        net: None,
                        busy: "",
                        note: None,
                    }));
                    // Swallow the 'g' that opened this screen so it doesn't land
                    // in the search box on the next frame.
                    while get_char_pressed().is_some() {}
                    next_frame().await;
                    continue;
                }
                // A broken song can't be launched — nudge, don't crash on its
                // empty difficulty list. (It's still selectable so it can be
                // deleted.) With an empty filter there's nothing to launch.
                if is_key_pressed(KeyCode::Enter) && view.is_empty() {
                    // nothing selected — the filter matched no songs
                } else if is_key_pressed(KeyCode::Enter) && songs[*sel].error.is_some() {
                    engine.play(&sounds.miss, 0.3);
                    toast = Some(Toast::new("this song failed to load - it can't be played"));
                } else if is_key_pressed(KeyCode::Enter) {
                    let (row, d) = (*sel, diff_opts[*diff_sel].0);
                    // Already choosing an instrument: launch it. Otherwise, if
                    // this difficulty charts two comparable instruments, open the
                    // chooser in the difficulty row and wait for the next Enter;
                    // a single-chart difficulty launches straight away.
                    let instrument = if pick.is_some() {
                        let (options, isel) = pick.as_ref().unwrap();
                        Some(options[*isel])
                    } else {
                        let options = chart::charts_for_diff(&songs[row].charts, d);
                        if options.len() > 1 {
                            *pick = Some((options, 0));
                            engine.play(&sounds.hat, 0.4);
                            None
                        } else {
                            Some(options.first().copied().unwrap_or(Instrument::Guitar))
                        }
                    };
                    if let Some(instrument) = instrument {
                        engine.play(&sounds.kick, 0.5);
                        toast = None;
                        let rx = spawn_loader(
                            songs[row].source.clone(),
                            instrument,
                            engine.sample_rate,
                            stem_cache.clone(),
                        );
                        scene = Scene::Loading {
                            rx,
                            song: row,
                            diff: d,
                            title: songs[row].title.clone(),
                        };
                        next_frame().await;
                        continue;
                    }
                }

                clear_background(th().bg);
                let k = ui();
                let t = get_time();
                let pulse = ((t * 2.0).sin() * 0.5 + 0.5) as f32;

                draw_centered(
                    "KEYBOARD WARRIOR",
                    130.0 * k,
                    72.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.95),
                );
                if searching {
                    // Search bar in the subtitle's slot: the live query with a
                    // blinking caret, and the match count off to the side. The
                    // wheel below is already filtered to `view`.
                    let q = menu_search.as_deref().unwrap_or("");
                    let box_w = (screen_width() * 0.5).min(600.0 * k);
                    let bx = screen_width() / 2.0 - box_w / 2.0;
                    let by = 156.0 * k;
                    let box_h = 34.0 * k;
                    draw_rectangle(bx, by, box_w, box_h, Color::new(1.0, 1.0, 1.0, 0.06));
                    draw_rectangle_lines(bx, by, box_w, box_h, 2.0 * k, wa(th().accent, 0.8));
                    let caret = if (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
                    let (shown, col) = if q.is_empty() {
                        ("search title or artist...".to_string(), Color::new(1.0, 1.0, 1.0, 0.3))
                    } else {
                        (format!("{q}{caret}"), Color::new(1.0, 1.0, 1.0, 0.9))
                    };
                    dtext(&shown, bx + 12.0 * k, by + 23.0 * k, 20.0 * k, col);
                    let n = view.len();
                    let side =
                        format!("{n} match{}  ·  ESC closes", if n == 1 { "" } else { "es" });
                    dtext(
                        &side,
                        bx + box_w + 14.0 * k,
                        by + 23.0 * k,
                        15.0 * k,
                        wa(th().secondary, 0.55),
                    );
                } else {
                    draw_centered(
                        "a rhythm typing game",
                        170.0 * k,
                        26.0 * k,
                        Color::new(0.35, 0.85, 1.0, 0.6 + 0.3 * pulse),
                    );
                }

                // Transient status message, tucked into the top-right corner
                // and faded out over its last moments (see TOAST_LIFE). Drop it
                // in a separate step so the draw below can borrow it.
                if toast.as_ref().is_some_and(|t| get_time() - t.born >= TOAST_LIFE) {
                    toast = None;
                }
                if let Some(t) = &toast {
                    let age = get_time() - t.born;
                    // Ease in over the first moment, hold, fade over the last ~0.7s.
                    let a = ((age / 0.12).min((TOAST_LIFE - age) / 0.7)).clamp(0.0, 1.0) as f32;
                    let size = 17.0 * k;
                    let dims = msize(&t.text, size);
                    let pad = 12.0 * k;
                    let w = dims.width + pad * 2.0;
                    let h = size + pad * 1.3;
                    let bx = screen_width() - w - 20.0 * k;
                    let by = 20.0 * k;
                    draw_rectangle(bx, by, w, h, wa(th().bg, 0.92 * a));
                    draw_rectangle(bx, by, w, h, Color::new(1.0, 1.0, 1.0, 0.06 * a));
                    draw_rectangle_lines(bx, by, w, h, 1.5 * k, wa(th().secondary, 0.45 * a));
                    dtext(
                        &t.text,
                        bx + pad,
                        by + h / 2.0 + dims.height / 2.0,
                        size,
                        wa(th().secondary, 0.95 * a),
                    );
                }

                // The song list is a wheel of bare titles, so many songs fit
                // in the band: the selected row expands in place to show the
                // artist and difficulty selector, pushing its neighbors
                // apart, and everything eases as the selection moves.
                let dtf = get_frame_time();
                // The wheel indexes into `view` (the filtered rows), so scroll
                // eases toward the selection's position within it, not its
                // full-list index — otherwise a filter would scroll off-screen.
                let sel_pos = view.iter().position(|&i| i == *sel).unwrap_or(0) as f32;
                *scroll += (sel_pos - *scroll) * (1.0 - (-dtf * 12.0).exp());
                // Every piece of chrome around the band is scaled, which is
                // what keeps the band itself usable. The header and the
                // legend/hints below it used to be fixed pixel blocks totalling
                // ~500 px, so a short window ate the song list from both ends —
                // under three songs by 590 px tall, and literally zero rows by
                // 452, with nothing on screen to say the list wasn't empty.
                // Scaling the chrome instead means it can never claim more than
                // its share, and since the row pitch scales with it the wheel
                // shows a near-constant ~6-7 songs at any height.
                let hint_top = screen_height() - FOOTER_H * k; // top of the legend
                let band_top = 222.0 * k;
                let spacing = 46.0 * k;
                // `ui()` stops shrinking at its floor, so a window small enough
                // to hit it can still squeeze the band shut. Keep one row's
                // worth open regardless: overlapping the legend reads as
                // cramped, where an empty wheel reads as "no songs installed".
                let band_bot = (hint_top - 26.0 * k).max(band_top + spacing);
                let cy = (band_top + band_bot) / 2.0;
                let expand = 76.0 * k; // extra room the selected row's details take
                for (pos, &row) in view.iter().enumerate() {
                    let song = &songs[row];
                    let off = pos as f32 - *scroll;
                    // Rows below the selection shift down by the expansion;
                    // centering it keeps the selected title on the band's axis
                    let shift = expand * (off + 0.5).clamp(0.0, 1.0) - expand / 2.0;
                    let y = cy + off * spacing + shift;
                    if y < band_top - 24.0 * k || y > band_bot + 24.0 * k {
                        continue;
                    }
                    // Wheel opacity: fade with distance from the center and
                    // extinguish completely at the band edges
                    let fade = 70.0 * k;
                    let edge = (((y - band_top) / fade).min((band_bot - y) / fade)).clamp(0.0, 1.0);
                    let a = (1.0 - off.abs() / 6.0).clamp(0.0, 1.0) * edge;
                    if a <= 0.02 {
                        continue;
                    }
                    // How settled the selection is on this row: grows the
                    // title and fades the details in as the wheel eases
                    let focus = (1.0 - off.abs()).clamp(0.0, 1.0);
                    let selected = row == *sel;
                    let size = (26.0 + 14.0 * focus) * k;
                    let name_color = if song.locked {
                        // Signpost rows sit in the wheel but read as inert
                        wa(th().secondary, 0.35 * a)
                    } else if song.error.is_some() {
                        // Broken songs read red so they're obviously unplayable
                        wa(th().miss, (if selected { 0.9 } else { 0.5 }) * a)
                    } else if selected {
                        wa(th().secondary, a)
                    } else {
                        Color::new(1.0, 1.0, 1.0, (0.40 + 0.15 * focus) * a)
                    };
                    if selected {
                        let dims = msize(&song.title, size);
                        dtext(
                            ">",
                            screen_width() / 2.0 - dims.width / 2.0 - 40.0 * k,
                            y,
                            size,
                            Color::new(1.0, 1.0, 1.0, (0.5 + 0.5 * pulse) * a * focus),
                        );
                    }
                    draw_centered(&song.title, y, size, name_color);
                    if selected && focus > 0.05 {
                        let fa = focus * a;
                        draw_centered(
                            &song.artist,
                            y + 26.0 * k,
                            18.0 * k,
                            Color::new(1.0, 1.0, 1.0, 0.55 * fa),
                        );
                        // Exactly one detail line sits under the artist: the
                        // delete prompt when this row is armed, else the song's
                        // load error, else the difficulty selector. Only ever
                        // one, so nothing stacks onto the row below it.
                        #[cfg(not(target_arch = "wasm32"))]
                        let arming_delete = pending_delete == Some(row);
                        #[cfg(target_arch = "wasm32")]
                        let arming_delete = false;
                        if arming_delete {
                            draw_centered(
                                "press DELETE again to remove  ·  any move cancels",
                                y + 52.0 * k,
                                17.0 * k,
                                wa(th().miss, 0.9 * fa),
                            );
                        } else if let Some(err) = &song.error {
                            draw_centered(
                                &format!("failed to load: {err}"),
                                y + 52.0 * k,
                                17.0 * k,
                                wa(th().miss, 0.85 * fa),
                            );
                        } else if let Some((options, isel)) = pick.as_ref() {
                            // The difficulty row has turned into a guitar/bass
                            // chooser for the chosen difficulty; each option
                            // carries its note count so they're easy to tell
                            // apart.
                            let d = diff_opts.get(*diff_sel).map(|&(d, _)| d).unwrap_or(0);
                            let joined: Vec<String> = options
                                .iter()
                                .enumerate()
                                .map(|(i, inst)| {
                                    let notes = song
                                        .charts
                                        .iter()
                                        .find(|c| c.instrument == *inst)
                                        .map(|c| c.counts[d])
                                        .unwrap_or(0);
                                    let label = format!("{} ({} notes)", inst.label(), notes);
                                    if i == *isel {
                                        format!("[ {label} ]")
                                    } else {
                                        label
                                    }
                                })
                                .collect();
                            draw_centered(
                                &joined.join("   "),
                                y + 52.0 * k,
                                20.0 * k,
                                wa(th().accent, 0.85 * fa),
                            );
                        } else {
                            let joined: Vec<String> = diff_opts
                                .iter()
                                .enumerate()
                                .map(|(i, (_, n))| {
                                    if i == *diff_sel {
                                        format!("[ {} ]", n)
                                    } else {
                                        n.to_string()
                                    }
                                })
                                .collect();
                            draw_centered(
                                &joined.join("   "),
                                y + 52.0 * k,
                                20.0 * k,
                                wa(th().accent, 0.85 * fa),
                            );
                        }
                    }
                    // Personal best for the highlighted difficulty, set into the
                    // right margin beside the title so it never crowds the wheel
                    // rows (which are packed too tight for a fourth detail line).
                    #[cfg(not(target_arch = "wasm32"))]
                    if selected && focus > 0.5 {
                        if let Some(b) = diff_opts.get(*diff_sel).and_then(|&(d, _)| {
                            scores.best(&song.title, &song.artist, d, words::text_mode_label())
                        }) {
                            let bsize = 15.0 * k;
                            let txt = format!("best {}  ·  {:.1}%", b.score, b.accuracy);
                            let dims = msize(&txt, bsize);
                            let pad = 5.0 * k;
                            let bx = screen_width() - FOOTER_INSET * k - pad - dims.width;
                            let title_right =
                                screen_width() / 2.0 + msize(&song.title, size).width / 2.0;
                            // Skip it if a long title would run into it.
                            if bx > title_right + 24.0 * k {
                                // A 100% (flawless) best turns gold and gets a
                                // border; anything less stays a quiet subtitle.
                                let perfect = b.accuracy >= 100.0;
                                if perfect {
                                    draw_rectangle_lines(
                                        bx - pad,
                                        y - dims.offset_y - pad,
                                        dims.width + pad * 2.0,
                                        dims.height + pad * 2.0,
                                        2.0 * k,
                                        wa(th().accent, 0.85 * a),
                                    );
                                }
                                let col = if perfect {
                                    wa(th().accent, a)
                                } else {
                                    wa(th().secondary, 0.6 * a)
                                };
                                dtext(&txt, bx, y, bsize, col);
                            }
                        }
                    }
                }
                // Filter matched nothing — say so where the wheel would be.
                if view.is_empty() {
                    let raw = menu_search.as_deref().unwrap_or("").trim();
                    draw_centered(
                        &format!("no songs match \"{raw}\""),
                        cy,
                        24.0 * k,
                        wa(th().secondary, 0.7),
                    );
                }

                // Footer, laid out from the bottom edge up: teach (legend and
                // the scoring line), then state, then controls. One rule
                // divides what the game does from what the player presses.
                let (stat_s, hint_s) = (Style::stat(k), Style::hint(k));
                let avail = screen_width() - FOOTER_INSET * 2.0 * k;
                let hint_cy = screen_height() - 30.0 * k - hint_s.height() / 2.0;
                let stat_cy = hint_cy - hint_s.height() / 2.0 - 22.0 * k - stat_s.height() / 2.0;
                let rule_y = stat_cy - stat_s.height() / 2.0 - 20.0 * k;
                let teach_y = rule_y - 24.0 * k;
                draw_keyboard_legend(screen_width() / 2.0, teach_y - 122.0 * k);
                draw_centered(
                    // The browser demo has no whammy bar (real app only)
                    if cfg!(target_arch = "wasm32") {
                        "gold gems build star power  ·  SPACE unleashes it"
                    } else {
                        "gold gems build star power  ·  SPACE unleashes it  ·  SHIFT for whammy bar"
                    },
                    teach_y,
                    20.0 * k,
                    wa(th().accent, 0.45),
                );
                draw_rule(rule_y, FOOTER_INSET * k, k);

                let mode_name =
                    TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].1;
                let off_ms = CALIB_MS.load(Ordering::Relaxed);
                let speed = SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].0;
                // The menu hotkeys, each shown with what it currently reads —
                // the same rows the settings screen carries, so a regular
                // never has to leave the menu to check or change them.
                let stats = [
                    Item::stat(Cap::Txt("M"), "MODE", mode_name),
                    Item::stat(Cap::Txt("V"), "SPEED", speed),
                    Item::stat(Cap::Txt("T"), "THEME", th().name),
                    Item::stat(Cap::Txt("C"), "OFFSET", format!("{off_ms:+} ms")),
                    Item::stat(
                        Cap::Pair("-", "+"),
                        "VOLUME",
                        format!("{:.0}%", engine.master() * 100.0),
                    ),
                ];
                draw_strip(&[&stats], stat_cy, avail, stat_s);

                // Arrow-key navigation isn't listed anywhere: the wheel and the
                // difficulty row both show their own selection, and arrows on a
                // list need no teaching.
                let play = [
                    Item::act(Cap::Txt("ENTER"), "play"),
                    Item::act(Cap::Txt(";"), "search"),
                    Item::act(Cap::Txt("S"), "settings"),
                ];
                // Library management is native only — the browser demo ships a
                // fixed library — and is the cluster a narrow window drops,
                // since every one of these is also reachable from settings.
                #[cfg(not(target_arch = "wasm32"))]
                let library = [
                    Item::act(Cap::Txt("A"), "add folder"),
                    Item::act(Cap::Txt("F"), "open folder"),
                    Item::act(Cap::Txt("R"), "rescan"),
                    Item::act(Cap::Txt("G"), "download"),
                    Item::act(Cap::Txt("DEL"), "remove"),
                ];
                #[cfg(not(target_arch = "wasm32"))]
                draw_strip(&[&play, &library], hint_cy, avail, hint_s);
                #[cfg(target_arch = "wasm32")]
                draw_strip(&[&play], hint_cy, avail, hint_s);
            }

            Scene::Settings { sel, menu_sel } => {
                let rows = settings_rows();
                *sel = (*sel).min(rows.len() - 1);
                if is_key_pressed(KeyCode::Escape) {
                    let m = *menu_sel;
                    scene = Scene::Menu { sel: m, diff_sel: 0, scroll: m as f32, pick: None };
                    next_frame().await;
                    continue;
                }
                let nav = nav_repeat.poll(&[KeyCode::Up, KeyCode::Down], get_frame_time());
                if nav == Some(KeyCode::Up) {
                    *sel = (*sel + rows.len() - 1) % rows.len();
                    engine.play(&sounds.hat, 0.4);
                }
                if nav == Some(KeyCode::Down) {
                    *sel = (*sel + 1) % rows.len();
                    engine.play(&sounds.hat, 0.4);
                }
                let row = rows[*sel];
                let dir =
                    is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
                if dir != 0 {
                    row.adjust(dir, &engine);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::Enter) {
                    if row == SettingRow::Calibrate {
                        engine.play(&sounds.kick, 0.4);
                        engine.start_timeline(1.0, None, None);
                        scene = Scene::Calibrate(Calibrate {
                            taps: Vec::new(),
                            scheduled_until: 0.0,
                            menu_sel: *menu_sel,
                            from_menu: false,
                            off_ms: CALIB_MS.load(Ordering::Relaxed),
                        });
                        next_frame().await;
                        continue;
                    }
                    // ENTER nudges any other row forward, so toggles feel right
                    row.adjust(1, &engine);
                    engine.play(&sounds.kick, 0.4);
                }
                // Cycling away from PRACTICE collapses its filter rows —
                // rebuild before drawing so this frame shows the new list
                let rows = settings_rows();
                *sel = (*sel).min(rows.len() - 1);
                let row = rows[*sel];

                clear_background(th().bg);
                let k = ui();
                let t = get_time();
                let pulse = ((t * 2.0).sin() * 0.5 + 0.5) as f32;
                draw_centered("SETTINGS", 130.0 * k, 56.0 * k, Color::new(1.0, 1.0, 1.0, 0.95));

                // The list is a two-column form on a fixed axis: labels flush
                // right of it, values flush left. Both columns stay put as
                // values change, so nothing on screen shifts while you hold a
                // key down cycling one.
                let axis = screen_width() / 2.0;
                let lines = settings_lines();
                let hint_s = Style::hint(k);
                let cap_s = Style::stat(k);
                // The footer is fixed; the list gets what's left, and its
                // pitch shrinks to fit rather than running underneath. Turning
                // PRACTICE on adds six rows, which is exactly when a short
                // window would otherwise overflow.
                let list_top = 200.0 * k;
                let list_bot = screen_height() - (30.0 + 26.0) * k - hint_s.height();
                let region = (list_bot - list_top).max(160.0 * k);
                // Row pitch in "units": a row claims one, a section heading
                // 1.25 with its air. The pitch shrinks to fit rather than the
                // list running under the footer — turning PRACTICE on adds six
                // rows, which is exactly when a short window would overflow.
                let secs = (lines.len() - rows.len()) as f32;
                let units = rows.len() as f32 + secs * 1.25;
                let desc_gap = 44.0 * k;
                let unit = ((region - desc_gap - 18.0 * k) / units).clamp(20.0 * k, 40.0 * k);
                let size = (unit * 0.55).min(22.0 * k);
                // List and description are centered in the region as one
                // block, so neither strands a hole when PRACTICE collapses.
                let block_h = units * unit;
                let top = list_top + ((region - (block_h + desc_gap + 18.0 * k)) / 2.0).max(0.0);
                // Shortcut column. Left-aligned and fixed, so the caps read as
                // a column rather than a ragged tail, and parked clear of the
                // longest value any row can show — `WORDS (FIXED)` — plus the
                // chevron drawn past it, so it can never collide or drift as
                // values change under it.
                let cap_col = axis + 232.0 * k;

                let mut y = top;
                for line in &lines {
                    match line {
                        SettingLine::Section(name) => {
                            // Headings hang left of the label column so they
                            // bracket the group without joining the form.
                            y += unit * 0.55;
                            let d = msize(name, 13.0 * k);
                            dtext(
                                name,
                                axis - 44.0 * k - d.width,
                                y,
                                13.0 * k,
                                wa(th().secondary, 0.5),
                            );
                            y += unit * 0.7;
                        }
                        SettingLine::Row(r) => {
                            let i = rows.iter().position(|x| x == r).unwrap_or(0);
                            let selected = i == *sel;
                            // Labels are right-aligned, so nesting has to
                            // mirror: a child steps *away* from the value
                            // column, not toward it. Shifting right would put
                            // the children closer to their values than their
                            // own parent, which reads as promotion.
                            let indent = if r.indented() { -24.0 * k } else { 0.0 };
                            let label_a = if selected {
                                0.95
                            } else if r.indented() {
                                0.42
                            } else {
                                0.62
                            };
                            let ld = msize(r.label(), size);
                            let lx = axis - 44.0 * k - ld.width + indent;
                            dtext(r.label(), lx, y, size, Color::new(1.0, 1.0, 1.0, label_a));
                            if selected {
                                dtext(
                                    ">",
                                    lx - 28.0 * k,
                                    y,
                                    size,
                                    Color::new(1.0, 1.0, 1.0, 0.5 + 0.5 * pulse),
                                );
                            }
                            // Value column. Toggles carry their state in the
                            // color as well as the word, so a glance down the
                            // column reads as a pattern of lit and unlit.
                            let v = r.value(&engine);
                            let vc = match (selected, r.toggle()) {
                                (true, _) => wa(th().accent, 0.95),
                                (false, Some(true)) => wa(th().accent, 0.6),
                                (false, Some(false)) => Color::new(1.0, 1.0, 1.0, 0.28),
                                (false, None) => wa(th().secondary, 0.55),
                            };
                            let vx = axis + 44.0 * k;
                            if selected {
                                // Chevrons sit outside the value so the value
                                // itself keeps the column's left edge.
                                let cd = msize("<", size);
                                dtext(
                                    "<",
                                    vx - cd.width - 10.0 * k,
                                    y,
                                    size,
                                    wa(th().accent, 0.45 + 0.35 * pulse),
                                );
                                dtext(&v, vx, y, size, vc);
                                let vd = msize(&v, size);
                                dtext(
                                    ">",
                                    vx + vd.width + 10.0 * k,
                                    y,
                                    size,
                                    wa(th().accent, 0.45 + 0.35 * pulse),
                                );
                            } else {
                                dtext(&v, vx, y, size, vc);
                            }
                            // The menu shortcut, in its own column past the
                            // values — present for whoever wants it, quiet
                            // enough to ignore while reading the form.
                            if let Some(cap) = r.hotkey() {
                                draw_inline_cap(
                                    cap,
                                    cap_col,
                                    y - msize("M", size).height / 2.0,
                                    cap_s,
                                );
                            }
                            y += unit;
                        }
                    }
                }

                // What the selected row does, sitting a fixed gap under the
                // list so it reads as the list's caption rather than as
                // footer chrome.
                draw_centered(row.desc(), y + desc_gap, 17.0 * k, wa(th().secondary, 0.75));
                // No arrow hints: the selected row's own `< value >` chevrons
                // say which way it moves, and stepping a list is self-evident.
                //
                // ENTER means different things per row — it nudges a value
                // forward, but on `calibrate` it opens the metronome — so the
                // hint names whichever one is actually under the cursor.
                let nav = [Item::act(
                    Cap::Txt("ENTER"),
                    if row == SettingRow::Calibrate { "calibrate" } else { "change" },
                )];
                let back = [Item::act(Cap::Txt("ESC"), "back")];
                draw_strip(
                    &[&nav, &back],
                    screen_height() - 30.0 * k - hint_s.height() / 2.0,
                    screen_width() - FOOTER_INSET * 2.0 * k,
                    hint_s,
                );
            }

            Scene::Playing(play) => {
                if is_key_pressed(KeyCode::Escape) {
                    play.paused = !play.paused;
                    if play.paused {
                        play.pause_now = engine.timeline_pos() - calib_offset();
                    }
                    engine.set_paused(play.paused);
                    engine.play(&sounds.hat, 0.4);
                }
                if play.paused {
                    if is_key_pressed(KeyCode::Q) {
                        engine.set_paused(false);
                        engine.stop_timeline();
                        let sel = play.song_ref.song;
                        let diff_sel = diff_pos(&songs, sel, play.song_ref.diff);
                        scene = Scene::Menu { sel, diff_sel, scroll: sel as f32, pick: None };
                        // Swallow the 'q' so a search bar left open on the menu
                        // doesn't pick it up as a typed character next frame.
                        while get_char_pressed().is_some() {}
                        next_frame().await;
                        continue;
                    }
                    // Restart: reload the same song+difficulty from the top. The
                    // stem cache holds this song's decoded audio, so the reload
                    // is instant — same path as the results screen's replay.
                    if is_key_pressed(KeyCode::R) {
                        engine.set_paused(false);
                        engine.stop_timeline();
                        engine.play(&sounds.kick, 0.5);
                        let SongRef { song, diff, instrument } = play.song_ref;
                        let rx = spawn_loader(
                            songs[song].source.clone(),
                            instrument,
                            engine.sample_rate,
                            stem_cache.clone(),
                        );
                        scene = Scene::Loading { rx, song, diff, title: songs[song].title.clone() };
                        next_frame().await;
                        continue;
                    }
                    // Keystrokes made while paused never reach judgement
                    while get_char_pressed().is_some() {}
                    play.draw(play.pause_now);
                    draw_rectangle(
                        0.0,
                        0.0,
                        screen_width(),
                        screen_height(),
                        Color::new(0.0, 0.0, 0.0, 0.55),
                    );
                    let k = ui();
                    draw_centered(
                        "PAUSED",
                        screen_height() * 0.42,
                        72.0 * k,
                        Color::new(1.0, 1.0, 1.0, 0.95),
                    );
                    let s = Style::hint(k);
                    draw_strip(
                        &[&[
                            Item::act(Cap::Txt("ESC"), "resume"),
                            Item::act(Cap::Txt("R"), "restart"),
                            Item::act(Cap::Txt("Q"), "quit to menu"),
                        ]],
                        screen_height() * 0.42 + 40.0 * k,
                        screen_width() - FOOTER_INSET * 2.0 * k,
                        s,
                    );
                    next_frame().await;
                    continue;
                }
                // The audio hardware's frame counter is the game clock; the
                // judged clock additionally carries the calibration offset.
                // The highway is drawn on the judged clock too, so a gem sits
                // on the strike line at the moment a perfect press is expected
                // — with headphone latency dialed in, that's also when you hear
                // the note. Only the real audio playback runs on the raw clock.
                let now = engine.timeline_pos();
                let jnow = now - calib_offset();
                while let Some(c) = get_char_pressed() {
                    play.handle_char(c, jnow, &sounds, &engine);
                }
                play.update(jnow, &sounds, &engine);
                play.draw(jnow);

                if play.finished(now) {
                    engine.stop_timeline();
                    // Record the run and see whether it set a personal best.
                    // Any new best is announced (a first clear included); the
                    // banner just drops the "+delta" when there's nothing prior.
                    #[cfg(not(target_arch = "wasm32"))]
                    let (new_best, prev_best) = {
                        let artist = songs[play.song_ref.song].artist.clone();
                        let rec = scores.record(
                            &play.title,
                            &artist,
                            play.song_ref.diff,
                            words::text_mode_label(),
                            config::BestScore {
                                score: play.score,
                                accuracy: play.accuracy(),
                                max_combo: play.max_combo,
                            },
                        );
                        (rec.improved, rec.prev_best)
                    };
                    #[cfg(target_arch = "wasm32")]
                    let (new_best, prev_best): (bool, Option<i64>) = (false, None);
                    scene = Scene::Results(Results {
                        song_ref: play.song_ref,
                        title: play.title.clone(),
                        diff_name: play.diff_name.clone(),
                        score: play.score,
                        max_combo: play.max_combo,
                        perfect: play.perfect,
                        great: play.great,
                        good: play.good,
                        miss: play.miss,
                        strays: play.strays,
                        accuracy: play.accuracy(),
                        new_best,
                        prev_best,
                    });
                }
            }

            Scene::Results(r) => {
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
                    let sel = r.song_ref.song;
                    let diff_sel = diff_pos(&songs, sel, r.song_ref.diff);
                    scene = Scene::Menu { sel, diff_sel, scroll: sel as f32, pick: None };
                    next_frame().await;
                    continue;
                }
                if is_key_pressed(KeyCode::R) {
                    engine.play(&sounds.kick, 0.5);
                    let SongRef { song, diff, instrument } = r.song_ref;
                    let rx = spawn_loader(
                        songs[song].source.clone(),
                        instrument,
                        engine.sample_rate,
                        stem_cache.clone(),
                    );
                    scene = Scene::Loading { rx, song, diff, title: songs[song].title.clone() };
                    next_frame().await;
                    continue;
                }

                clear_background(th().bg);
                let k = ui();
                let (grade, gcolor) = r.grade();
                draw_centered(grade, 220.0 * k, 160.0 * k, gcolor);
                draw_centered(
                    &format!("{}  ·  {}", r.title, r.diff_name),
                    270.0 * k,
                    26.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.5),
                );

                draw_centered(&format!("{}", r.score), 350.0 * k, 56.0 * k, WHITE);
                draw_centered(
                    &format!("{:.1}% acc   ·   {} max combo", r.accuracy, r.max_combo),
                    395.0 * k,
                    24.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.7),
                );
                // Personal-best banner: only when this run beat a prior score.
                if r.new_best {
                    let pulse = ((get_time() * 3.0).sin() * 0.5 + 0.5) as f32;
                    let msg = match r.prev_best {
                        Some(prev) => format!("NEW PERSONAL BEST!   +{}", r.score - prev),
                        None => "NEW PERSONAL BEST!".to_string(),
                    };
                    draw_centered(&msg, 424.0 * k, 22.0 * k, wa(th().accent, 0.6 + 0.4 * pulse));
                }

                let rows = [
                    ("PERFECT", r.perfect, Judgement::Perfect.color()),
                    ("GREAT", r.great, Judgement::Great.color()),
                    ("GOOD", r.good, Judgement::Good.color()),
                    ("MISS", r.miss, th().miss),
                    ("STRAY KEYS", r.strays, Color::new(1.0, 1.0, 1.0, 0.4)),
                ];
                for (i, (label, count, color)) in rows.iter().enumerate() {
                    let y = (460.0 + i as f32 * 34.0) * k;
                    let text = format!("{:<11} {:>4}", label, count);
                    draw_centered(&text, y, 26.0 * k, *color);
                }

                let s = Style::hint(k);
                draw_strip(
                    &[&[
                        Item::act(Cap::Txt("R"), "play again"),
                        Item::act(Cap::Txt("ENTER"), "menu"),
                    ]],
                    screen_height() - 30.0 * k - s.height() / 2.0,
                    screen_width() - FOOTER_INSET * 2.0 * k,
                    s,
                );
            }

            Scene::Loading { rx, song, diff, title } => {
                match rx.try_recv() {
                    Ok(LoadMsg::Done(loaded)) => {
                        let (song, diff) = (*song, *diff);
                        let LoadedSong { chart, backing, lead } = *loaded;
                        stem_cache = Some((
                            songs[song].source.clone(),
                            chart.instrument,
                            backing.clone(),
                            lead.clone(),
                        ));
                        // Fall back to the hardest charted difficulty if the
                        // requested one is empty or trivial
                        let mut d = diff.min(3);
                        if chart.diffs[d].len() < 20 {
                            if let Some(best) = (0..4).rev().find(|&i| chart.diffs[i].len() >= 20) {
                                d = best;
                            }
                        }
                        let play =
                            Play::new_chart(song, d, &chart, &engine, &sounds, backing, lead);
                        toast = None;
                        scene = Scene::Playing(Box::new(play));
                    }
                    Ok(LoadMsg::Failed(e)) => {
                        toast = Some(Toast::new(format!("{title}: {e}")));
                        let sel = *song;
                        scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32, pick: None };
                    }
                    Err(TryRecvError::Disconnected) => {
                        toast = Some(Toast::new(format!("{title}: loader thread died")));
                        let sel = *song;
                        scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32, pick: None };
                    }
                    Err(TryRecvError::Empty) => {
                        // Still decoding on the worker thread: keep animating
                        clear_background(th().bg);
                        let k = ui();
                        draw_centered(
                            "loading",
                            screen_height() * 0.44 - 40.0 * k,
                            20.0 * k,
                            wa(th().secondary, 0.75),
                        );
                        draw_centered(title, screen_height() * 0.44, 30.0 * k, WHITE);
                        let bw = 280.0 * k;
                        let bx = screen_width() / 2.0 - bw / 2.0;
                        let by = screen_height() * 0.5;
                        draw_rectangle(bx, by, bw, 4.0 * k, Color::new(1.0, 1.0, 1.0, 0.12));
                        let ph = ((get_time() * 0.8) % 1.0) as f32;
                        let sw = 90.0 * k;
                        let sx = bx - sw + (bw + sw) * ph;
                        let (x0, x1) = (sx.max(bx), (sx + sw).min(bx + bw));
                        if x1 > x0 {
                            draw_rectangle(x0, by, x1 - x0, 4.0 * k, wa(th().accent, 0.9));
                        }
                    }
                }
            }

            Scene::Calibrate(cal) => {
                let now = engine.timeline_pos();
                // Both exits land back where the player came from: the menu
                // (C hotkey) or the settings screen's calibrate row
                let back = if cal.from_menu {
                    Scene::Menu { sel: cal.menu_sel, diff_sel: 0, scroll: cal.menu_sel as f32, pick: None }
                } else {
                    Scene::Settings {
                        sel: settings_rows()
                            .iter()
                            .position(|r| *r == SettingRow::Calibrate)
                            .unwrap_or(0),
                        menu_sel: cal.menu_sel,
                    }
                };
                if is_key_pressed(KeyCode::Escape) {
                    engine.stop_timeline();
                    scene = back;
                    next_frame().await;
                    continue;
                }
                // Fine-tune step for hand-nudging the offset with Left/Right.
                const STEP_MS: i64 = 5;
                if is_key_pressed(KeyCode::Enter) {
                    CALIB_MS.store(cal.off_ms, Ordering::Relaxed);
                    engine.stop_timeline();
                    engine.play(&sounds.kick, 0.5);
                    scene = back;
                    next_frame().await;
                    continue;
                }
                if is_key_pressed(KeyCode::R) {
                    cal.taps.clear();
                    cal.off_ms = 0;
                    while get_char_pressed().is_some() {}
                    next_frame().await;
                    continue;
                }
                // Left/Right nudge the offset by hand — fine-tuning after the
                // taps land you close, or dialing it in from scratch without
                // tapping at all. Same key convention as the settings rows.
                let dir =
                    is_key_pressed(KeyCode::Right) as i64 - is_key_pressed(KeyCode::Left) as i64;
                if dir != 0 {
                    cal.off_ms = (cal.off_ms + dir * STEP_MS).clamp(-500, 500);
                    engine.play(&sounds.hat, 0.4);
                }
                // Any letter (or space) is a tap; offset vs the nearest tick.
                // The running tap median drives the offset directly, so the
                // number tracks live as you play along.
                while let Some(c) = get_char_pressed() {
                    let c = c.to_ascii_lowercase();
                    if (is_typeable(c) || c == ' ') && now > -0.25 {
                        let nearest = (now / CALIB_PERIOD).round() * CALIB_PERIOD;
                        cal.taps.push(now - nearest);
                        if cal.taps.len() > 24 {
                            cal.taps.remove(0);
                        }
                        cal.off_ms = (median(&cal.taps) * 1000.0).round() as i64;
                    }
                }
                // Keep the metronome stocked a few ticks ahead
                if now > cal.scheduled_until - 3.0 {
                    for i in 0..8 {
                        engine.play_at(
                            &sounds.hat,
                            0.8,
                            cal.scheduled_until + i as f64 * CALIB_PERIOD,
                        );
                    }
                    cal.scheduled_until += 8.0 * CALIB_PERIOD;
                }

                clear_background(th().bg);
                let k = ui();
                draw_centered("CALIBRATION", 120.0 * k, 48.0 * k, Color::new(1.0, 1.0, 1.0, 0.95));
                draw_centered(
                    "tap any letter key on each tick, or set the offset by hand",
                    162.0 * k,
                    20.0 * k,
                    wa(th().secondary, 0.8),
                );
                let (cx, cyy) = (screen_width() / 2.0, screen_height() * 0.40);
                if now.is_finite() {
                    let ph = (now.rem_euclid(CALIB_PERIOD) / CALIB_PERIOD) as f32;
                    draw_circle_lines(
                        cx,
                        cyy,
                        (30.0 + 40.0 * ph) * k,
                        3.0 * k,
                        wa(th().accent, 1.0 - 0.85 * ph),
                    );
                }
                draw_circle(cx, cyy, 10.0 * k, wa(th().accent, 0.9));

                // Tap scatter: early taps land left of center, late taps right
                let aw = 320.0 * k;
                let ay = screen_height() * 0.60;
                draw_line(
                    cx - aw / 2.0,
                    ay,
                    cx + aw / 2.0,
                    ay,
                    2.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.15),
                );
                let tick = 10.0 * k;
                draw_line(cx, ay - tick, cx, ay + tick, 2.0 * k, Color::new(1.0, 1.0, 1.0, 0.3));
                dtext(
                    "early",
                    cx - aw / 2.0 - 4.0 * k,
                    ay + 26.0 * k,
                    15.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
                let ld = msize("late", 15.0 * k);
                dtext(
                    "late",
                    cx + aw / 2.0 - ld.width + 4.0 * k,
                    ay + 26.0 * k,
                    15.0 * k,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
                let n = cal.taps.len();
                for (i, d) in cal.taps.iter().enumerate() {
                    let x =
                        (cx + (*d as f32 / 0.150) * (aw / 2.0)).clamp(cx - aw / 2.0, cx + aw / 2.0);
                    let a = 0.2 + 0.6 * (i + 1) as f32 / n as f32;
                    draw_circle(x, ay, 4.0 * k, Color::new(1.0, 1.0, 1.0, a));
                }
                // The committed offset — where a perfect hit will sit relative
                // to the beat. Driven by the tap median or nudged by hand; the
                // marker pins it against the tap scatter. Always shown, since
                // the offset can now be set with the arrows and no taps at all.
                let off_s = cal.off_ms as f32 / 1000.0;
                let mx = (cx + (off_s / 0.150) * (aw / 2.0)).clamp(cx - aw / 2.0, cx + aw / 2.0);
                let arm = 14.0 * k;
                draw_line(mx, ay - arm, mx, ay + arm, 3.0 * k, wa(th().accent, 0.9));
                let taps_note = if n > 0 { format!("   ({n} taps)") } else { String::new() };
                draw_centered(
                    &format!("offset {:+} ms{taps_note}", cal.off_ms),
                    ay + 58.0 * k,
                    24.0 * k,
                    wa(th().accent, 0.9),
                );
                // A few taps still get you in the ballpark fastest, so nudge
                // toward tapping until there's enough to take a median — but
                // the arrows can set the offset on their own, so nothing is
                // gated on it and the full footer shows from the start.
                let s = Style::hint(k);
                let left = 4usize.saturating_sub(cal.taps.len());
                if left > 0 {
                    let tick = if left == 1 { "tick" } else { "ticks" };
                    draw_centered(
                        &format!("tap along with {left} more {tick} for a quick estimate"),
                        screen_height() - 62.0 * k - s.height(),
                        18.0 * k,
                        wa(th().secondary, 0.7),
                    );
                }
                let adjust = [Item::act(Cap::Pair("-", "+"), "adjust")];
                let apply = [Item::act(Cap::Txt("ENTER"), "apply")];
                let reset = [Item::act(Cap::Txt("R"), "reset to 0")];
                let cancel = [Item::act(Cap::Txt("ESC"), "cancel")];
                let cy = screen_height() - 30.0 * k - s.height() / 2.0;
                let avail = screen_width() - FOOTER_INSET * 2.0 * k;
                draw_strip(&[&adjust, &apply, &reset, &cancel], cy, avail, s);
            }

            #[cfg(not(target_arch = "wasm32"))]
            Scene::Chorus(cs) => {
                // Drain a completed network job, if one was in flight.
                if let Some(rx) = &cs.net {
                    match rx.try_recv() {
                        Ok(ChorusMsg::Results(res)) => {
                            cs.busy = "";
                            cs.net = None;
                            match res {
                                Ok(hits) => {
                                    cs.note = (hits.is_empty()).then(|| "no matches".to_string());
                                    cs.hits = hits;
                                    cs.sel = 0;
                                    cs.scroll = 0;
                                }
                                Err(e) => cs.note = Some(e),
                            }
                        }
                        Ok(ChorusMsg::Downloaded(res)) => {
                            cs.busy = "";
                            cs.net = None;
                            match res {
                                Ok(title) => {
                                    // Freshly downloaded — rescan so it's playable
                                    // now, then drop back to the menu on it.
                                    songs = chart::scan_all(&song_roots(&config));
                                    let at = songs
                                        .iter()
                                        .position(|x| x.title == title && !x.locked)
                                        .unwrap_or(0);
                                    toast =
                                        Some(Toast::new(format!("added {title} to your library")));
                                    scene =
                                        Scene::Menu { sel: at, diff_sel: 0, scroll: at as f32, pick: None };
                                    next_frame().await;
                                    continue;
                                }
                                Err(e) => cs.note = Some(e),
                            }
                        }
                        Err(TryRecvError::Disconnected) => {
                            cs.busy = "";
                            cs.net = None;
                            cs.note = Some("network thread died".into());
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                }

                let idle = cs.net.is_none();
                if is_key_pressed(KeyCode::Escape) {
                    let m = cs.menu_sel;
                    scene = Scene::Menu { sel: m, diff_sel: 0, scroll: m as f32, pick: None };
                    next_frame().await;
                    continue;
                }
                // Input only while nothing is in flight.
                if idle {
                    // Kick off a search with the current query + difficulty
                    // filter, on a worker thread. Defined as a closure so both
                    // Enter and the left/right filter keys can trigger it.
                    let start_search = |cs: &mut ChorusScene| {
                        let q = cs.query.trim().to_string();
                        if q.is_empty() {
                            return;
                        }
                        let diff = chorus::DIFF_FILTERS[cs.diff_idx].1.map(str::to_string);
                        cs.busy = "searching...";
                        cs.note = None;
                        cs.focus = ChorusFocus::Search;
                        let (tx, rx) = channel();
                        std::thread::spawn(move || {
                            let _ =
                                tx.send(ChorusMsg::Results(chorus::search(&q, 1, diff.as_deref())));
                        });
                        cs.net = Some(rx);
                    };

                    // Cycle the difficulty filter and re-run the search so the
                    // change lands immediately, keeping the caller's focus so it
                    // works both while typing and while browsing results.
                    let cycle_diff = |cs: &mut ChorusScene, dir: i32| {
                        let n = chorus::DIFF_FILTERS.len();
                        cs.diff_idx = (cs.diff_idx as i32 + dir).rem_euclid(n as i32) as usize;
                        engine.play(&sounds.kick, 0.4);
                        if !cs.query.trim().is_empty() {
                            let focus = cs.focus; // start_search forces Search
                            start_search(cs);
                            cs.focus = focus;
                        }
                    };

                    // Tab flips focus between the query bar and the results.
                    if is_key_pressed(KeyCode::Tab) && !cs.hits.is_empty() {
                        cs.focus = match cs.focus {
                            ChorusFocus::Search => ChorusFocus::Results,
                            ChorusFocus::Results => ChorusFocus::Search,
                        };
                        engine.play(&sounds.hat, 0.4);
                    }

                    match cs.focus {
                        ChorusFocus::Search => {
                            // Only printable ASCII reaches the query — the font
                            // atlas holds ' '..='~' only, so arrow keys and other
                            // special keys (which come through get_char_pressed as
                            // non-printable chars) would otherwise draw as tofu.
                            while let Some(c) = get_char_pressed() {
                                if (' '..='~').contains(&c) {
                                    cs.query.push(c);
                                }
                            }
                            if is_key_pressed(KeyCode::Backspace) {
                                cs.query.pop();
                            }
                            // Left/right cycle the difficulty filter — bare
                            // letters can't, since every letter is query text.
                            let dir = is_key_pressed(KeyCode::Right) as i32
                                - is_key_pressed(KeyCode::Left) as i32;
                            if dir != 0 {
                                cycle_diff(cs, dir);
                            }
                            if is_key_pressed(KeyCode::Enter) {
                                start_search(cs);
                            }
                            // Down drops into the results list, if any.
                            if is_key_pressed(KeyCode::Down) && !cs.hits.is_empty() {
                                cs.focus = ChorusFocus::Results;
                                cs.sel = 0;
                                cs.scroll = 0;
                                engine.play(&sounds.hat, 0.4);
                            }
                        }
                        ChorusFocus::Results => {
                            // Drain typed chars so they don't queue up for when
                            // focus returns to the search box.
                            while get_char_pressed().is_some() {}
                            // Left/right re-filter by difficulty without leaving
                            // the results list (focus is preserved).
                            let dir = is_key_pressed(KeyCode::Right) as i32
                                - is_key_pressed(KeyCode::Left) as i32;
                            if dir != 0 {
                                cycle_diff(cs, dir);
                            }
                            let nav =
                                nav_repeat.poll(&[KeyCode::Up, KeyCode::Down], get_frame_time());
                            if nav == Some(KeyCode::Up) {
                                if cs.sel == 0 {
                                    cs.focus = ChorusFocus::Search; // back to the box
                                } else {
                                    cs.sel -= 1;
                                }
                                engine.play(&sounds.hat, 0.4);
                            }
                            if nav == Some(KeyCode::Down) && cs.sel + 1 < cs.hits.len() {
                                cs.sel += 1;
                                engine.play(&sounds.hat, 0.4);
                            }
                            // Pan the window to wherever the selection went
                            let (_, _, visible) = chorus_list_geom(ui());
                            if cs.sel < cs.scroll {
                                cs.scroll = cs.sel;
                            } else if cs.sel >= cs.scroll + visible {
                                cs.scroll = cs.sel + 1 - visible;
                            }
                            if is_key_pressed(KeyCode::Enter) && cs.sel < cs.hits.len() {
                                // Download the highlighted hit.
                                let hit = &cs.hits[cs.sel];
                                let owned = chorus::Hit {
                                    name: hit.name.clone(),
                                    artist: hit.artist.clone(),
                                    charter: hit.charter.clone(),
                                    md5: hit.md5.clone(),
                                    diff_guitar: hit.diff_guitar,
                                };
                                let title = if owned.name.is_empty() {
                                    "song".to_string()
                                } else {
                                    owned.name.clone()
                                };
                                cs.busy = "downloading...";
                                cs.note = None;
                                engine.play(&sounds.kick, 0.4);
                                let (tx, rx) = channel();
                                std::thread::spawn(move || {
                                    let dest = std::path::Path::new("songs");
                                    let res = chorus::download(&owned, dest).map(|_| title);
                                    let _ = tx.send(ChorusMsg::Downloaded(res));
                                });
                                cs.net = Some(rx);
                            }
                        }
                    }
                }

                // ---- draw ----
                clear_background(th().bg);
                let k = ui();
                draw_centered("GET SONGS", 96.0 * k, 48.0 * k, Color::new(1.0, 1.0, 1.0, 0.95));
                draw_centered(
                    "search Chorus Encore  ·  full chart (every charted difficulty) downloads into your songs folder",
                    136.0 * k,
                    18.0 * k,
                    wa(th().secondary, 0.8),
                );
                // External-source disclaimer: these files come from a third party.
                draw_centered(
                    "charts are community uploads from enchor.us  ·  download at your own risk",
                    160.0 * k,
                    15.0 * k,
                    Color::new(1.0, 0.8, 0.4, 0.55),
                );
                let cx = screen_width() / 2.0;

                // Search box
                let box_w = (screen_width() * 0.6).min(680.0 * k);
                let bx = cx - box_w / 2.0;
                let by = 192.0 * k;
                let box_h = 40.0 * k;
                let searching_focus = cs.focus == ChorusFocus::Search;
                let box_edge = if searching_focus { 0.8 } else { 0.35 };
                draw_rectangle(bx, by, box_w, box_h, Color::new(1.0, 1.0, 1.0, 0.06));
                draw_rectangle_lines(bx, by, box_w, box_h, 2.0 * k, wa(th().accent, box_edge));
                let caret = if idle && searching_focus && (get_time() * 2.0) as i64 % 2 == 0 {
                    "_"
                } else {
                    ""
                };
                let placeholder = cs.query.is_empty() && searching_focus;
                let shown = if placeholder {
                    "type a song or artist...".to_string()
                } else {
                    format!("{}{caret}", cs.query)
                };
                let q_color = if placeholder {
                    Color::new(1.0, 1.0, 1.0, 0.3)
                } else {
                    Color::new(1.0, 1.0, 1.0, 0.9)
                };
                dtext(&shown, bx + 14.0 * k, by + 27.0 * k, 22.0 * k, q_color);

                // Difficulty filter, to the right of the box.
                let (dname, _) = chorus::DIFF_FILTERS[cs.diff_idx];
                dtext(
                    &format!("< > difficulty: {dname}"),
                    bx,
                    by + 62.0 * k,
                    17.0 * k,
                    wa(th().secondary, 0.75),
                );

                // Busy / status line
                if !cs.busy.is_empty() {
                    draw_centered(cs.busy, by + 92.0 * k, 20.0 * k, wa(th().accent, 0.9));
                } else if let Some(note) = &cs.note {
                    draw_centered(note, by + 92.0 * k, 18.0 * k, wa(th().miss, 0.8));
                }

                // Results, panned so the selection is always on screen
                let (list_top, row_h, visible) = chorus_list_geom(k);
                // Growing the window fits more rows than the offset assumed —
                // pull it back so the list can't sit with a blank tail
                cs.scroll = cs.scroll.min(cs.hits.len().saturating_sub(visible));

                // Selection band, bracketed to the two lines of the row from
                // the font's own metrics: `dtext` puts a string's ink in
                // `Rect(x, y - offset_y, w, h)`, so a band hung off the title's
                // baseline sits too high — it left a gap above the title and
                // cut the subtitle's descenders. Measured off a fixed sample
                // carrying both an ascender and a descender, so every row's
                // band is identical no matter what the row happens to say.
                let (band_top, band_h) = {
                    let pad = 3.0 * k;
                    let name_m = msize("Ag", 22.0 * k);
                    let sub_m = msize("Ag", 16.0 * k);
                    let top = -name_m.offset_y - pad;
                    let bot = 22.0 * k - sub_m.offset_y + sub_m.height + pad;
                    // Never let the band close the gap to the next row, however
                    // the font's metrics come out
                    (top, (bot - top).min(row_h - 6.0 * k))
                };
                for (i, hit) in cs.hits.iter().enumerate().skip(cs.scroll) {
                    let y = list_top + (i - cs.scroll) as f32 * row_h;
                    if y > screen_height() - 90.0 * k {
                        break;
                    }
                    let selected =
                        i == cs.sel && cs.focus == ChorusFocus::Results && cs.net.is_none();
                    if selected {
                        draw_rectangle(bx, y + band_top, box_w, band_h, wa(th().accent, 0.10));
                    }
                    let name_a = if selected { 0.95 } else { 0.7 };
                    dtext(&hit.name, bx + 14.0 * k, y, 22.0 * k, Color::new(1.0, 1.0, 1.0, name_a));
                    let sub = if hit.charter.is_empty() {
                        hit.artist.clone()
                    } else {
                        format!("{}   ·   charter: {}", hit.artist, hit.charter)
                    };
                    dtext(&sub, bx + 14.0 * k, y + 22.0 * k, 16.0 * k, wa(th().secondary, 0.7));
                }

                // Slim track down the right edge once the list overflows —
                // without it there's no cue that results continue past the fold.
                if cs.hits.len() > visible {
                    let (tx, ty) = (bx + box_w + 8.0 * k, list_top - 22.0 * k);
                    let track_h = visible as f32 * row_h;
                    let frac = visible as f32 / cs.hits.len() as f32;
                    let thumb_h = (track_h * frac).max(18.0 * k);
                    let pos = cs.scroll as f32 / (cs.hits.len() - visible) as f32;
                    let w = 3.0 * k;
                    draw_rectangle(tx, ty, w, track_h, Color::new(1.0, 1.0, 1.0, 0.10));
                    draw_rectangle(
                        tx,
                        ty + (track_h - thumb_h) * pos,
                        w,
                        thumb_h,
                        wa(th().secondary, 0.7),
                    );
                }

                // The footer tracks focus: whichever half of the screen has the
                // keyboard is the half whose keys get listed first.
                let s = Style::hint(k);
                let searching = cs.focus == ChorusFocus::Search;
                let primary: Vec<Item> = if cs.busy.is_empty() && !searching {
                    vec![
                        Item::act(Cap::Txt("ENTER"), "download"),
                        Item::act(Cap::Txt("TAB"), "edit search"),
                    ]
                } else if cs.busy.is_empty() {
                    let mut v = vec![Item::act(Cap::Txt("ENTER"), "search")];
                    if !cs.hits.is_empty() {
                        v.push(Item::act(Cap::Txt("TAB"), "browse results"));
                    }
                    v
                } else {
                    Vec::new()
                };
                // The difficulty filter isn't listed: it carries its own
                // `< >` affordance inline, right under the search box.
                let back = [Item::act(Cap::Txt("ESC"), "back")];
                let cy = screen_height() - 30.0 * k - s.height() / 2.0;
                let avail = screen_width() - FOOTER_INSET * 2.0 * k;
                if primary.is_empty() {
                    draw_strip(&[&back], cy, avail, s);
                } else {
                    draw_strip(&[&primary, &back], cy, avail, s);
                }
            }
        }

        // Volume overlay: bottom-left, fading out after the last press
        if vol_flash > 0.0 {
            vol_flash -= get_frame_time();
            let a = (vol_flash / 0.4).clamp(0.0, 1.0);
            let v = engine.master();
            let k = ui();
            let (bx, by, bw) = (24.0 * k, screen_height() - 46.0 * k, 150.0 * k);
            dtext(
                &format!("VOLUME {:.0}%", v * 100.0),
                bx,
                by - 10.0 * k,
                16.0 * k,
                Color::new(1.0, 1.0, 1.0, 0.75 * a),
            );
            draw_rectangle(bx, by, bw, 6.0 * k, Color::new(1.0, 1.0, 1.0, 0.12 * a));
            draw_rectangle(bx, by, bw * v, 6.0 * k, wa(th().secondary, 0.85 * a));
        }
        if show_frame_graph {
            draw_frame_graph(&frame_log);
        }
        // Persist any settings change made this frame, from whichever scene.
        // Cheap: the snapshot is a handful of scalars and a write only happens
        // when something actually changed.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let snap = config::settings_snapshot(&engine);
            if snap != last_settings {
                config::save_settings(&snap);
                last_settings = snap;
            }
        }
        next_frame().await;
    }
}
