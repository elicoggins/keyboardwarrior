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
use chart::{SongChart, SongSource, DIFF_NAMES};
use gfx::{draw_centered, draw_fit, draw_frame_graph, dtext, msize, prewarm_glyphs, FRAME_LOG_LEN};
use play::{is_typeable, lane_of, Judgement, Play, Results, SongRef};
use settings::{
    calib_offset, cycle, flip, settings_rows, sustains_on, SettingRow, CALIB_MS, CALIB_PERIOD,
    SPEEDS, SPEED_IDX, SUSTAINS,
};
use theme::{th, wa, THEMES, THEME_IDX};
use words::{TEXT_MODES, TEXT_MODE_IDX};

/// Mini keyboard legend drawn in the menu: every key tinted by its lane, so
/// the lane-to-hand mapping is shown, not spelled out.
fn draw_keyboard_legend(center_x: f32, top_y: f32) {
    let rows: [(&str, f32); 3] = [("qwertyuiop", 0.0), ("asdfghjkl;", 0.4), ("zxcvbnm,.", 1.0)];
    let key = 26.0;
    let gap = 5.0;
    let full = 10.0 * (key + gap) - gap;
    for (ri, (row, stagger)) in rows.iter().enumerate() {
        let y = top_y + ri as f32 * (key + gap);
        let x0 = center_x - full / 2.0 + stagger * (key + gap) * 0.5;
        for (ci, ch) in row.chars().enumerate() {
            let x = x0 + ci as f32 * (key + gap);
            let c = th().lane[lane_of(ch)];
            draw_rectangle(x, y, key, key, wa(c, 0.14));
            draw_rectangle_lines(x, y, key, key, 1.5, wa(c, 0.45));
            let label = ch.to_ascii_uppercase().to_string();
            let d = msize(&label, 13.0);
            dtext(
                &label,
                x + key / 2.0 - d.width / 2.0,
                y + key / 2.0 + d.height / 2.0,
                13.0,
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
}

enum Scene {
    Menu {
        sel: usize,
        diff_sel: usize,
        scroll: f32,
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
    query: String,                    // the text being typed
    focus: ChorusFocus,               // query bar vs results list
    diff_idx: usize,                  // index into chorus::DIFF_FILTERS
    hits: Vec<chorus::Hit>,           // results of the last search
    sel: usize,                       // highlighted result
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

type StemCache = Option<(SongSource, Buf, Option<Buf>)>;

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
fn spawn_loader(source: SongSource, rate: u32, cached: StemCache) -> Receiver<LoadMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let msg = match load_song_full(&source, rate, cached) {
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
fn spawn_loader(source: SongSource, rate: u32, cached: StemCache) -> Receiver<LoadMsg> {
    let (tx, rx) = channel();
    web::defer(move || {
        let msg = match load_song_full(&source, rate, cached) {
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
fn load_song_full(source: &SongSource, rate: u32, cached: StemCache) -> Result<LoadedSong, String> {
    let chart = chart::load_song(source)?;
    if let Some((src, backing, lead)) = cached {
        if src == *source {
            return Ok(LoadedSong { chart, backing, lead });
        }
    }
    let stems = chart::stem_files(source)?;
    if stems.is_empty() {
        return Err("no audio stems found".into());
    }
    let lead_names = chart::lead_stem_names(chart.instrument);
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
    #[cfg(not(target_arch = "wasm32"))]
    let song_roots = |cfg: &config::Config| -> Vec<std::path::PathBuf> {
        let mut roots = vec![std::path::PathBuf::from("songs")];
        roots.extend(cfg.song_dirs.iter().cloned());
        roots
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (mut songs, mut scan_errors) = chart::scan_all(&song_roots(&config));
    #[cfg(target_arch = "wasm32")]
    let (songs, scan_errors) = web::load_demo_library().await;
    let mut stem_cache: StemCache = None;
    // The most recent load failure, shown in the menu until the next attempt
    let mut status_error: Option<String> = None;
    // Song index armed for deletion: Delete once arms the selected row, Delete
    // again on the same row removes it. Any navigation disarms.
    #[cfg(not(target_arch = "wasm32"))]
    let mut pending_delete: Option<usize> = None;
    let mut scene = Scene::Menu { sel: 0, diff_sel: 0, scroll: 0.0 };

    // Debug hook: KW_AUTOSTART=<song>:<diff> jumps straight into a song
    if let Ok(v) = std::env::var("KW_AUTOSTART") {
        let mut it = v.split(':');
        let s: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let d: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        if s < songs.len() {
            let rx = spawn_loader(songs[s].source.clone(), engine.sample_rate, None);
            scene = Scene::Loading { rx, song: s, diff: d, title: songs[s].title.clone() };
        }
    }

    // Frame-time overlay (F1), for chasing stutter by eye
    let mut show_frame_graph = false;
    let mut frame_log: std::collections::VecDeque<f32> =
        std::collections::VecDeque::with_capacity(FRAME_LOG_LEN);
    // Seconds left on the master-volume overlay after a -/+ press
    let mut vol_flash = 0.0f32;

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
            Scene::Menu { sel, diff_sel, scroll } => {
                let rows = songs.len();
                if rows == 0 {
                    clear_background(th().bg);
                    draw_centered("KEYBOARD WARRIOR", 130.0, 72.0, Color::new(1.0, 1.0, 1.0, 0.95));
                    draw_centered(
                        if cfg!(target_arch = "wasm32") {
                            "demo song unavailable - refresh the page to retry"
                        } else {
                            "no songs found - drop a Clone Hero .sng or song folder into songs/"
                        },
                        screen_height() * 0.5,
                        22.0,
                        wa(th().secondary, 0.8),
                    );
                    // If everything in songs/ failed to load, say why
                    for (i, e) in scan_errors.iter().take(6).enumerate() {
                        draw_centered(
                            e,
                            screen_height() * 0.5 + 44.0 + i as f32 * 22.0,
                            17.0,
                            wa(th().miss, 0.75),
                        );
                    }
                    next_frame().await;
                    continue;
                }
                // Difficulty options for the selected song
                let diff_opts: Vec<(usize, String)> =
                    songs[*sel].available.iter().map(|&d| (d, DIFF_NAMES[d].to_string())).collect();
                *diff_sel = (*diff_sel).min(diff_opts.len() - 1);

                if is_key_pressed(KeyCode::Up) {
                    *sel = (*sel + rows - 1) % rows;
                    // Locked signpost rows are never selectable
                    while songs[*sel].locked {
                        *sel = (*sel + rows - 1) % rows;
                    }
                    *diff_sel = 0;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Down) {
                    *sel = (*sel + 1) % rows;
                    while songs[*sel].locked {
                        *sel = (*sel + 1) % rows;
                    }
                    *diff_sel = 0;
                    engine.play(&sounds.hat, 0.4);
                }
                // Moving off a row cancels a pending deletion on it.
                #[cfg(not(target_arch = "wasm32"))]
                if (is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Down))
                    && pending_delete.is_some_and(|d| d != *sel)
                {
                    pending_delete = None;
                }
                if is_key_pressed(KeyCode::Left) && *diff_sel > 0 {
                    *diff_sel -= 1;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Right) && *diff_sel + 1 < diff_opts.len() {
                    *diff_sel += 1;
                    engine.play(&sounds.hat, 0.4);
                }
                // Hotkeys for the common settings, mirrored on the settings
                // screen (O) — regulars shouldn't need to leave the menu
                if is_key_pressed(KeyCode::M) {
                    cycle(&TEXT_MODE_IDX, TEXT_MODES.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::T) {
                    cycle(&THEME_IDX, THEMES.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::S) {
                    flip(&SUSTAINS);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::V) {
                    cycle(&SPEED_IDX, SPEEDS.len(), 1);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::C) {
                    engine.play(&sounds.kick, 0.4);
                    engine.start_timeline(1.0, None, None);
                    scene = Scene::Calibrate(Calibrate {
                        taps: Vec::new(),
                        scheduled_until: 0.0,
                        menu_sel: *sel,
                        from_menu: true,
                    });
                    next_frame().await;
                    continue;
                }
                if is_key_pressed(KeyCode::O) {
                    engine.play(&sounds.kick, 0.4);
                    scene = Scene::Settings { sel: 0, menu_sel: *sel };
                    next_frame().await;
                    continue;
                }
                // Song-library management (native only — the browser demo has a
                // fixed library and no filesystem).
                #[cfg(not(target_arch = "wasm32"))]
                if is_key_pressed(KeyCode::A) {
                    engine.play(&sounds.kick, 0.4);
                    // The picker is modal and blocks the render loop; that's
                    // fine — the player is deliberately paused on a dialog.
                    if let Some(dir) =
                        rfd::FileDialog::new().set_title("Add a song folder").pick_folder()
                    {
                        let shown = dir.display().to_string();
                        if config.add_song_dir(dir) {
                            let (s, e) = chart::scan_all(&song_roots(&config));
                            songs = s;
                            scan_errors = e;
                            *sel = 0;
                            *scroll = 0.0;
                            status_error =
                                Some(format!("added {shown} - {} songs total", songs.len()));
                        } else {
                            status_error = Some(format!("{shown} is already in your library"));
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if is_key_pressed(KeyCode::F) {
                    engine.play(&sounds.kick, 0.4);
                    if let Err(e) = open_in_file_manager(std::path::Path::new("songs")) {
                        status_error = Some(format!("couldn't open songs folder: {e}"));
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if is_key_pressed(KeyCode::R) {
                    engine.play(&sounds.kick, 0.4);
                    let (s, e) = chart::scan_all(&song_roots(&config));
                    songs = s;
                    scan_errors = e;
                    *sel = (*sel).min(songs.len().saturating_sub(1));
                    pending_delete = None;
                    status_error = Some(format!("rescanned - {} songs", songs.len()));
                }
                // Delete: two presses on the same row remove the song from disk.
                #[cfg(not(target_arch = "wasm32"))]
                if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
                    let row = *sel;
                    if chart::is_bundled(&songs[row].source) {
                        status_error = Some("bundled default songs can't be deleted".into());
                        pending_delete = None;
                    } else if pending_delete == Some(row) {
                        // Confirmed — remove it, then rescan and fix selection.
                        let title = songs[row].title.clone();
                        match chart::delete_song(&songs[row].source) {
                            Ok(()) => {
                                let (s, e) = chart::scan_all(&song_roots(&config));
                                songs = s;
                                scan_errors = e;
                                *sel = row.min(songs.len().saturating_sub(1));
                                status_error = Some(format!("deleted {title}"));
                                engine.play(&sounds.kick, 0.5);
                            }
                            Err(e) => status_error = Some(format!("couldn't delete {title}: {e}")),
                        }
                        pending_delete = None;
                    } else {
                        // Arm it: the row now shows a confirm prompt.
                        pending_delete = Some(row);
                        engine.play(&sounds.hat, 0.4);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if is_key_pressed(KeyCode::G) {
                    engine.play(&sounds.kick, 0.4);
                    scene = Scene::Chorus(Box::new(ChorusScene {
                        query: String::new(),
                        focus: ChorusFocus::Search,
                        diff_idx: 0,
                        hits: Vec::new(),
                        sel: 0,
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
                if is_key_pressed(KeyCode::Enter) {
                    engine.play(&sounds.kick, 0.5);
                    status_error = None;
                    let (row, d) = (*sel, diff_opts[*diff_sel].0);
                    let rx = spawn_loader(
                        songs[row].source.clone(),
                        engine.sample_rate,
                        stem_cache.clone(),
                    );
                    scene =
                        Scene::Loading { rx, song: row, diff: d, title: songs[row].title.clone() };
                    next_frame().await;
                    continue;
                }

                clear_background(th().bg);
                let t = get_time();
                let pulse = ((t * 2.0).sin() * 0.5 + 0.5) as f32;

                draw_centered("KEYBOARD WARRIOR", 130.0, 72.0, Color::new(1.0, 1.0, 1.0, 0.95));
                draw_centered(
                    "a rhythm typing game",
                    170.0,
                    26.0,
                    Color::new(0.35, 0.85, 1.0, 0.6 + 0.3 * pulse),
                );

                // Songs that failed to scan, small in the top-left corner
                for (i, e) in scan_errors.iter().take(3).enumerate() {
                    dtext(
                        &format!("! {e}"),
                        16.0,
                        28.0 + i as f32 * 20.0,
                        15.0,
                        wa(th().miss, 0.55),
                    );
                }
                if scan_errors.len() > 3 {
                    let more = format!("  + {} more", scan_errors.len() - 3);
                    dtext(&more, 16.0, 28.0 + 60.0, 15.0, wa(th().miss, 0.4));
                }
                // The last load failure, front and center
                if let Some(err) = &status_error {
                    draw_centered(&format!("!  {err}"), 205.0, 17.0, wa(th().miss, 0.85));
                }

                // The song list is a wheel of bare titles, so many songs fit
                // in the band: the selected row expands in place to show the
                // artist and difficulty selector, pushing its neighbors
                // apart, and everything eases as the selection moves.
                let dtf = get_frame_time();
                *scroll += (*sel as f32 - *scroll) * (1.0 - (-dtf * 12.0).exp());
                let hint_top = screen_height() - 130.0 - 122.0; // keyboard legend top
                let band_top = 222.0;
                let band_bot = hint_top - 26.0;
                let cy = (band_top + band_bot) / 2.0;
                let spacing = 46.0;
                let expand = 76.0; // extra room the selected row's details take
                for (row, song) in songs.iter().enumerate() {
                    let off = row as f32 - *scroll;
                    // Rows below the selection shift down by the expansion;
                    // centering it keeps the selected title on the band's axis
                    let shift = expand * (off + 0.5).clamp(0.0, 1.0) - expand / 2.0;
                    let y = cy + off * spacing + shift;
                    if y < band_top - 24.0 || y > band_bot + 24.0 {
                        continue;
                    }
                    // Wheel opacity: fade with distance from the center and
                    // extinguish completely at the band edges
                    let edge = (((y - band_top) / 70.0).min((band_bot - y) / 70.0)).clamp(0.0, 1.0);
                    let a = (1.0 - off.abs() / 6.0).clamp(0.0, 1.0) * edge;
                    if a <= 0.02 {
                        continue;
                    }
                    // How settled the selection is on this row: grows the
                    // title and fades the details in as the wheel eases
                    let focus = (1.0 - off.abs()).clamp(0.0, 1.0);
                    let selected = row == *sel;
                    let size = 26.0 + 14.0 * focus;
                    let name_color = if song.locked {
                        // Signpost rows sit in the wheel but read as inert
                        wa(th().secondary, 0.35 * a)
                    } else if selected {
                        wa(th().secondary, a)
                    } else {
                        Color::new(1.0, 1.0, 1.0, (0.40 + 0.15 * focus) * a)
                    };
                    if selected {
                        let dims = msize(&song.title, size);
                        dtext(
                            ">",
                            screen_width() / 2.0 - dims.width / 2.0 - 40.0,
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
                            y + 26.0,
                            18.0,
                            Color::new(1.0, 1.0, 1.0, 0.55 * fa),
                        );
                        // Difficulty selector, only for the selected song
                        let joined: Vec<String> =
                            diff_opts
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
                            y + 52.0,
                            20.0,
                            wa(th().accent, 0.85 * fa),
                        );
                        // Delete confirmation, in place of nothing else — a
                        // second Delete on this row removes it.
                        #[cfg(not(target_arch = "wasm32"))]
                        if pending_delete == Some(row) {
                            draw_centered(
                                "press DELETE again to remove this song  ·  any move cancels",
                                y + 76.0,
                                17.0,
                                wa(th().miss, 0.9 * fa),
                            );
                        }
                    }
                }

                let hint_y = screen_height() - 130.0;
                draw_keyboard_legend(screen_width() / 2.0, hint_y - 122.0);
                draw_centered(
                    // The browser demo has no whammy bar (real app only)
                    if cfg!(target_arch = "wasm32") {
                        "gold gems build star power  ·  SPACE unleashes it"
                    } else {
                        "gold gems build star power  ·  SPACE unleashes it  ·  SHIFT for whammy bar"
                    },
                    hint_y + 28.0,
                    20.0,
                    wa(th().accent, 0.45),
                );
                let mode_name =
                    TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].1;
                let sus = if sustains_on() { "ON" } else { "OFF" };
                let off_ms = CALIB_MS.load(Ordering::Relaxed);
                let speed = SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].0;
                draw_fit(
                    &format!(
                        "M · text: {}   ·   T · theme: {}   ·   S · sustains: {}   ·   V · speed: {}   ·   C · calibrate ({off_ms:+} ms)   ·   -/+ · volume: {:.0}%",
                        mode_name,
                        th().name,
                        sus,
                        speed,
                        engine.master() * 100.0
                    ),
                    screen_width() / 2.0,
                    hint_y + 56.0,
                    20.0,
                    screen_width() - 32.0,
                    wa(th().secondary, 0.7),
                );
                draw_centered(
                    // The library keys (add/open/rescan/get) are native only —
                    // the browser demo ships a fixed library.
                    if cfg!(target_arch = "wasm32") {
                        "up/down: song   ·   left/right: difficulty   ·   enter: play   ·   O: all settings"
                    } else {
                        "up/down: song · left/right: difficulty · enter: play · O: settings · A: add folder · F: open songs · R: rescan · G: get songs · del: remove"
                    },
                    hint_y + 84.0,
                    18.0,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
            }

            Scene::Settings { sel, menu_sel } => {
                let rows = settings_rows();
                *sel = (*sel).min(rows.len() - 1);
                if is_key_pressed(KeyCode::Escape) {
                    let m = *menu_sel;
                    scene = Scene::Menu { sel: m, diff_sel: 0, scroll: m as f32 };
                    next_frame().await;
                    continue;
                }
                if is_key_pressed(KeyCode::Up) {
                    *sel = (*sel + rows.len() - 1) % rows.len();
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Down) {
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
                let t = get_time();
                let pulse = ((t * 2.0).sin() * 0.5 + 0.5) as f32;
                draw_centered("SETTINGS", 130.0, 56.0, Color::new(1.0, 1.0, 1.0, 0.95));
                let cx = screen_width() / 2.0;
                let top = 210.0;
                let spacing = 40.0;
                for (i, r) in rows.iter().enumerate() {
                    let y = top + i as f32 * spacing;
                    let selected = i == *sel;
                    let indent = if r.indented() { 26.0 } else { 0.0 };
                    let size = 22.0;
                    let label_a = if selected {
                        0.95
                    } else if r.indented() {
                        0.42
                    } else {
                        0.60
                    };
                    let ld = msize(r.label(), size);
                    let lx = cx - 44.0 - ld.width + indent;
                    dtext(r.label(), lx, y, size, Color::new(1.0, 1.0, 1.0, label_a));
                    if selected {
                        dtext(
                            ">",
                            lx - 30.0,
                            y,
                            size,
                            Color::new(1.0, 1.0, 1.0, 0.5 + 0.5 * pulse),
                        );
                    }
                    let v = r.value(&engine);
                    if selected {
                        dtext(&format!("< {} >", v), cx + 44.0, y, size, wa(th().accent, 0.95));
                    } else {
                        dtext(&v, cx + 60.0, y, size, wa(th().secondary, 0.55));
                    }
                }
                draw_centered(
                    row.desc(),
                    top + rows.len() as f32 * spacing + 34.0,
                    17.0,
                    wa(th().secondary, 0.7),
                );
                draw_centered(
                    "up/down: select   ·   left/right: change   ·   esc: back",
                    screen_height() - 60.0,
                    18.0,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
            }

            Scene::Playing(play) => {
                if is_key_pressed(KeyCode::Escape) {
                    play.paused = !play.paused;
                    if play.paused {
                        play.pause_now = engine.timeline_pos();
                    }
                    engine.set_paused(play.paused);
                    engine.play(&sounds.hat, 0.4);
                }
                if play.paused {
                    if is_key_pressed(KeyCode::Q) {
                        engine.set_paused(false);
                        engine.stop_timeline();
                        let sel = play.song_ref.song;
                        scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32 };
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
                    draw_centered(
                        "PAUSED",
                        screen_height() * 0.42,
                        72.0,
                        Color::new(1.0, 1.0, 1.0, 0.95),
                    );
                    draw_centered(
                        "esc: resume   ·   q: quit to menu",
                        screen_height() * 0.42 + 44.0,
                        22.0,
                        Color::new(1.0, 1.0, 1.0, 0.55),
                    );
                    next_frame().await;
                    continue;
                }
                // The audio hardware's frame counter is the game clock; the
                // judged clock additionally carries the calibration offset
                let now = engine.timeline_pos();
                let jnow = now - calib_offset();
                while let Some(c) = get_char_pressed() {
                    play.handle_char(c, jnow, &sounds, &engine);
                }
                play.update(now, jnow, &sounds, &engine);
                play.draw(now);

                if play.finished(now) {
                    engine.stop_timeline();
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
                    });
                }
            }

            Scene::Results(r) => {
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
                    let sel = r.song_ref.song;
                    scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32 };
                    next_frame().await;
                    continue;
                }
                if is_key_pressed(KeyCode::R) {
                    engine.play(&sounds.kick, 0.5);
                    let SongRef { song, diff } = r.song_ref;
                    let rx = spawn_loader(
                        songs[song].source.clone(),
                        engine.sample_rate,
                        stem_cache.clone(),
                    );
                    scene = Scene::Loading { rx, song, diff, title: songs[song].title.clone() };
                    next_frame().await;
                    continue;
                }

                clear_background(th().bg);
                let (grade, gcolor) = r.grade();
                draw_centered(grade, 220.0, 160.0, gcolor);
                draw_centered(
                    &format!("{}  ·  {}", r.title, r.diff_name),
                    270.0,
                    26.0,
                    Color::new(1.0, 1.0, 1.0, 0.5),
                );

                draw_centered(&format!("{}", r.score), 350.0, 56.0, WHITE);
                draw_centered(
                    &format!("{:.1}% acc   ·   {} max combo", r.accuracy, r.max_combo),
                    395.0,
                    24.0,
                    Color::new(1.0, 1.0, 1.0, 0.7),
                );

                let rows = [
                    ("PERFECT", r.perfect, Judgement::Perfect.color()),
                    ("GREAT", r.great, Judgement::Great.color()),
                    ("GOOD", r.good, Judgement::Good.color()),
                    ("MISS", r.miss, th().miss),
                    ("STRAY KEYS", r.strays, Color::new(1.0, 1.0, 1.0, 0.4)),
                ];
                for (i, (label, count, color)) in rows.iter().enumerate() {
                    let y = 460.0 + i as f32 * 34.0;
                    let text = format!("{:<11} {:>4}", label, count);
                    draw_centered(&text, y, 26.0, *color);
                }

                draw_centered(
                    "R to play again   ·   enter for menu",
                    680.0,
                    20.0,
                    Color::new(1.0, 1.0, 1.0, 0.4),
                );
            }

            Scene::Loading { rx, song, diff, title } => {
                match rx.try_recv() {
                    Ok(LoadMsg::Done(loaded)) => {
                        let (song, diff) = (*song, *diff);
                        let LoadedSong { chart, backing, lead } = *loaded;
                        stem_cache =
                            Some((songs[song].source.clone(), backing.clone(), lead.clone()));
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
                        status_error = None;
                        scene = Scene::Playing(Box::new(play));
                    }
                    Ok(LoadMsg::Failed(e)) => {
                        status_error = Some(format!("{title}: {e}"));
                        let sel = *song;
                        scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32 };
                    }
                    Err(TryRecvError::Disconnected) => {
                        status_error = Some(format!("{title}: loader thread died"));
                        let sel = *song;
                        scene = Scene::Menu { sel, diff_sel: 0, scroll: sel as f32 };
                    }
                    Err(TryRecvError::Empty) => {
                        // Still decoding on the worker thread: keep animating
                        clear_background(th().bg);
                        draw_centered(
                            "loading",
                            screen_height() * 0.44 - 40.0,
                            20.0,
                            wa(th().secondary, 0.75),
                        );
                        draw_centered(title, screen_height() * 0.44, 30.0, WHITE);
                        let bw = 280.0;
                        let bx = screen_width() / 2.0 - bw / 2.0;
                        let by = screen_height() * 0.5;
                        draw_rectangle(bx, by, bw, 4.0, Color::new(1.0, 1.0, 1.0, 0.12));
                        let ph = ((get_time() * 0.8) % 1.0) as f32;
                        let sw = 90.0;
                        let sx = bx - sw + (bw + sw) * ph;
                        let (x0, x1) = (sx.max(bx), (sx + sw).min(bx + bw));
                        if x1 > x0 {
                            draw_rectangle(x0, by, x1 - x0, 4.0, wa(th().accent, 0.9));
                        }
                    }
                }
            }

            Scene::Calibrate(cal) => {
                let now = engine.timeline_pos();
                // Both exits land back where the player came from: the menu
                // (C hotkey) or the settings screen's calibrate row
                let back = if cal.from_menu {
                    Scene::Menu { sel: cal.menu_sel, diff_sel: 0, scroll: cal.menu_sel as f32 }
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
                let ready = cal.taps.len() >= 4;
                if is_key_pressed(KeyCode::Enter) && ready {
                    let ms = (median(&cal.taps) * 1000.0).round() as i64;
                    CALIB_MS.store(ms, Ordering::Relaxed);
                    engine.stop_timeline();
                    engine.play(&sounds.kick, 0.5);
                    scene = back;
                    next_frame().await;
                    continue;
                }
                // Any letter (or space) is a tap; offset vs the nearest tick
                while let Some(c) = get_char_pressed() {
                    let c = c.to_ascii_lowercase();
                    if (is_typeable(c) || c == ' ') && now > -0.25 {
                        let nearest = (now / CALIB_PERIOD).round() * CALIB_PERIOD;
                        cal.taps.push(now - nearest);
                        if cal.taps.len() > 24 {
                            cal.taps.remove(0);
                        }
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
                draw_centered("CALIBRATION", 120.0, 48.0, Color::new(1.0, 1.0, 1.0, 0.95));
                draw_centered(
                    "tap any letter key exactly on each tick",
                    162.0,
                    20.0,
                    wa(th().secondary, 0.8),
                );
                let (cx, cyy) = (screen_width() / 2.0, screen_height() * 0.40);
                if now.is_finite() {
                    let ph = (now.rem_euclid(CALIB_PERIOD) / CALIB_PERIOD) as f32;
                    draw_circle_lines(
                        cx,
                        cyy,
                        30.0 + 40.0 * ph,
                        3.0,
                        wa(th().accent, 1.0 - 0.85 * ph),
                    );
                }
                draw_circle(cx, cyy, 10.0, wa(th().accent, 0.9));

                // Tap scatter: early taps land left of center, late taps right
                let aw = 320.0;
                let ay = screen_height() * 0.60;
                draw_line(
                    cx - aw / 2.0,
                    ay,
                    cx + aw / 2.0,
                    ay,
                    2.0,
                    Color::new(1.0, 1.0, 1.0, 0.15),
                );
                draw_line(cx, ay - 10.0, cx, ay + 10.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.3));
                dtext(
                    "early",
                    cx - aw / 2.0 - 4.0,
                    ay + 26.0,
                    15.0,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
                let ld = msize("late", 15.0);
                dtext(
                    "late",
                    cx + aw / 2.0 - ld.width + 4.0,
                    ay + 26.0,
                    15.0,
                    Color::new(1.0, 1.0, 1.0, 0.3),
                );
                let n = cal.taps.len();
                for (i, d) in cal.taps.iter().enumerate() {
                    let x =
                        (cx + (*d as f32 / 0.150) * (aw / 2.0)).clamp(cx - aw / 2.0, cx + aw / 2.0);
                    let a = 0.2 + 0.6 * (i + 1) as f32 / n as f32;
                    draw_circle(x, ay, 4.0, Color::new(1.0, 1.0, 1.0, a));
                }
                if !cal.taps.is_empty() {
                    let m = median(&cal.taps);
                    let mx =
                        (cx + (m as f32 / 0.150) * (aw / 2.0)).clamp(cx - aw / 2.0, cx + aw / 2.0);
                    draw_line(mx, ay - 14.0, mx, ay + 14.0, 3.0, wa(th().accent, 0.9));
                    draw_centered(
                        &format!("offset {:+.0} ms   ({} taps)", m * 1000.0, n),
                        ay + 58.0,
                        24.0,
                        wa(th().accent, 0.9),
                    );
                }
                draw_centered(
                    if ready {
                        "enter: apply   ·   esc: cancel"
                    } else {
                        "tap along with at least 4 ticks   ·   esc: cancel"
                    },
                    screen_height() - 80.0,
                    20.0,
                    Color::new(1.0, 1.0, 1.0, 0.45),
                );
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
                                    let (s, e) = chart::scan_all(&song_roots(&config));
                                    songs = s;
                                    scan_errors = e;
                                    let at = songs
                                        .iter()
                                        .position(|x| x.title == title && !x.locked)
                                        .unwrap_or(0);
                                    status_error = Some(format!("added {title} to your library"));
                                    scene = Scene::Menu { sel: at, diff_sel: 0, scroll: at as f32 };
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
                    scene = Scene::Menu { sel: m, diff_sel: 0, scroll: m as f32 };
                    next_frame().await;
                    continue;
                }
                // Input only while nothing is in flight.
                if idle {
                    // Kick off a search with the current query + difficulty
                    // filter, on a worker thread. Defined as a closure so both
                    // Enter and the D-filter key can trigger it.
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

                    // Tab flips focus between the query bar and the results.
                    if is_key_pressed(KeyCode::Tab) && !cs.hits.is_empty() {
                        cs.focus = match cs.focus {
                            ChorusFocus::Search => ChorusFocus::Results,
                            ChorusFocus::Results => ChorusFocus::Search,
                        };
                        engine.play(&sounds.hat, 0.4);
                    }
                    // D cycles the difficulty filter and re-runs the search so
                    // the change takes effect immediately.
                    if is_key_pressed(KeyCode::D) {
                        cs.diff_idx = (cs.diff_idx + 1) % chorus::DIFF_FILTERS.len();
                        engine.play(&sounds.kick, 0.4);
                        start_search(cs);
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
                            if is_key_pressed(KeyCode::Enter) {
                                start_search(cs);
                            }
                            // Down drops into the results list, if any.
                            if is_key_pressed(KeyCode::Down) && !cs.hits.is_empty() {
                                cs.focus = ChorusFocus::Results;
                                cs.sel = 0;
                                engine.play(&sounds.hat, 0.4);
                            }
                        }
                        ChorusFocus::Results => {
                            // Drain typed chars so they don't queue up for when
                            // focus returns to the search box.
                            while get_char_pressed().is_some() {}
                            if is_key_pressed(KeyCode::Up) {
                                if cs.sel == 0 {
                                    cs.focus = ChorusFocus::Search; // back to the box
                                } else {
                                    cs.sel -= 1;
                                }
                                engine.play(&sounds.hat, 0.4);
                            }
                            if is_key_pressed(KeyCode::Down) && cs.sel + 1 < cs.hits.len() {
                                cs.sel += 1;
                                engine.play(&sounds.hat, 0.4);
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
                draw_centered("GET SONGS", 96.0, 48.0, Color::new(1.0, 1.0, 1.0, 0.95));
                draw_centered(
                    "search Chorus Encore  ·  full chart (every charted difficulty) downloads into your songs folder",
                    136.0,
                    18.0,
                    wa(th().secondary, 0.8),
                );
                // External-source disclaimer: these files come from a third party.
                draw_centered(
                    "charts are community uploads from enchor.us  ·  download at your own risk",
                    160.0,
                    15.0,
                    Color::new(1.0, 0.8, 0.4, 0.55),
                );
                let cx = screen_width() / 2.0;

                // Search box
                let box_w = (screen_width() * 0.6).min(680.0);
                let bx = cx - box_w / 2.0;
                let by = 192.0;
                let searching_focus = cs.focus == ChorusFocus::Search;
                let box_edge = if searching_focus { 0.8 } else { 0.35 };
                draw_rectangle(bx, by, box_w, 40.0, Color::new(1.0, 1.0, 1.0, 0.06));
                draw_rectangle_lines(bx, by, box_w, 40.0, 2.0, wa(th().accent, box_edge));
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
                dtext(&shown, bx + 14.0, by + 27.0, 22.0, q_color);

                // Difficulty filter, to the right of the box.
                let (dname, _) = chorus::DIFF_FILTERS[cs.diff_idx];
                dtext(
                    &format!("[D] guitar difficulty: {dname}"),
                    bx,
                    by + 62.0,
                    17.0,
                    wa(th().secondary, 0.75),
                );

                // Busy / status line
                if !cs.busy.is_empty() {
                    draw_centered(cs.busy, by + 92.0, 20.0, wa(th().accent, 0.9));
                } else if let Some(note) = &cs.note {
                    draw_centered(note, by + 92.0, 18.0, wa(th().miss, 0.8));
                }

                // Results
                let list_top = by + 128.0;
                let row_h = 52.0;
                for (i, hit) in cs.hits.iter().enumerate() {
                    let y = list_top + i as f32 * row_h;
                    if y > screen_height() - 90.0 {
                        break;
                    }
                    let selected =
                        i == cs.sel && cs.focus == ChorusFocus::Results && cs.net.is_none();
                    if selected {
                        draw_rectangle(bx, y - 22.0, box_w, row_h - 8.0, wa(th().accent, 0.10));
                    }
                    let name_a = if selected { 0.95 } else { 0.7 };
                    dtext(&hit.name, bx + 14.0, y, 22.0, Color::new(1.0, 1.0, 1.0, name_a));
                    let sub = if hit.charter.is_empty() {
                        hit.artist.clone()
                    } else {
                        format!("{}   ·   charter: {}", hit.artist, hit.charter)
                    };
                    dtext(&sub, bx + 14.0, y + 22.0, 16.0, wa(th().secondary, 0.7));
                    // Guitar tier the chart tops out at, right-aligned in the row.
                    let tier = format!("guitar: {}", chorus::guitar_tier_name(hit.diff_guitar));
                    let td = msize(&tier, 16.0);
                    dtext(&tier, bx + box_w - td.width - 14.0, y, 16.0, wa(th().accent, 0.7));
                }

                let hint = if !cs.busy.is_empty() {
                    "working...  ·  esc: back"
                } else if cs.hits.is_empty() {
                    "type a query  ·  enter: search  ·  D: difficulty  ·  esc: back"
                } else if cs.focus == ChorusFocus::Search {
                    "enter: search  ·  down/tab: browse results  ·  D: difficulty  ·  esc: back"
                } else {
                    "up/down: pick  ·  enter: download  ·  tab: edit search  ·  D: difficulty  ·  esc: back"
                };
                draw_centered(hint, screen_height() - 56.0, 18.0, Color::new(1.0, 1.0, 1.0, 0.4));
            }
        }

        // Volume overlay: bottom-left, fading out after the last press
        if vol_flash > 0.0 {
            vol_flash -= get_frame_time();
            let a = (vol_flash / 0.4).clamp(0.0, 1.0);
            let v = engine.master();
            let (bx, by, bw) = (24.0, screen_height() - 46.0, 150.0);
            dtext(
                &format!("VOLUME {:.0}%", v * 100.0),
                bx,
                by - 10.0,
                16.0,
                Color::new(1.0, 1.0, 1.0, 0.75 * a),
            );
            draw_rectangle(bx, by, bw, 6.0, Color::new(1.0, 1.0, 1.0, 0.12 * a));
            draw_rectangle(bx, by, bw * v, 6.0, wa(th().secondary, 0.85 * a));
        }
        if show_frame_graph {
            draw_frame_graph(&frame_log);
        }
        next_frame().await;
    }
}
