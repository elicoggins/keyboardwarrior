// Browser-demo support (wasm32 only). The demo ships one song —
// Code Monkey, fetched over HTTP as a .sng with vorbis stems — plus a locked
// "download to expand library" signpost row in the menu.

use std::cell::RefCell;
use std::sync::Arc;

use macroquad::experimental::coroutines::start_coroutine;
use macroquad::prelude::*;

use crate::chart::{self, SongEntry, SongSource};
use crate::gfx::{draw_centered, ui};
use crate::theme::{th, wa};

const DEMO_SONG_URL: &str = "songs/Code Monkey.sng";

/// Fetch the demo song and build the menu library, animating a download
/// screen while the multi-megabyte .sng arrives.
pub async fn load_demo_library() -> Vec<SongEntry> {
    let fetch = start_coroutine(async { macroquad::file::load_file(DEMO_SONG_URL).await });
    loop {
        if let Some(res) = fetch.retrieve() {
            return match res {
                Ok(bytes) => demo_entries(bytes),
                Err(_) => Vec::new(),
            };
        }
        clear_background(th().bg);
        // The canvas is sized to the browser viewport, so this screen sees a
        // wider spread of sizes than the desktop build ever does
        let k = ui();
        draw_centered("KEYBOARD WARRIOR", 130.0 * k, 72.0 * k, Color::new(1.0, 1.0, 1.0, 0.95));
        draw_centered(
            "downloading demo song",
            screen_height() * 0.44,
            22.0 * k,
            wa(th().secondary, 0.75),
        );
        // The same indeterminate sweep the song-loading scene uses
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
        next_frame().await;
    }
}

fn demo_entries(bytes: Vec<u8>) -> Vec<SongEntry> {
    let source = SongSource::Bytes(Arc::new(bytes));
    match chart::load_song(&source) {
        Ok(charts) => {
            let available = chart::available_diffs(&charts);
            if available.is_empty() {
                return Vec::new();
            }
            let (title, artist) =
                charts.first().map(|c| (c.title.clone(), c.artist.clone())).unwrap_or_default();
            let song = SongEntry {
                title: if title.is_empty() { "Code Monkey".into() } else { title },
                artist,
                available,
                charts: chart::chart_infos(&charts),
                sections: charts.first().map(|c| c.sections.clone()).unwrap_or_default(),
                source,
                locked: false,
                error: None,
            };
            let signpost = SongEntry {
                title: "download to expand library".into(),
                artist: String::new(),
                available: Vec::new(),
                charts: Vec::new(),
                sections: Vec::new(),
                source: SongSource::Bytes(Arc::new(Vec::new())),
                locked: true,
                error: None,
            };
            vec![song, signpost]
        }
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------- loader pump
//
// There are no threads on wasm, so song decoding runs on the game thread.
// Jobs are deferred by two frames so the loading screen has been drawn AND
// presented before the synchronous decode blocks the event loop; the frozen
// frame the player stares at is the loading screen, not the menu.

type Job = Box<dyn FnOnce()>;

extern "C" {
    // Pause/resume the demo's Web Audio context (web/kw_audio.js) around a
    // blocking decode — see pump() for why.
    fn kw_audio_suspend();
    fn kw_audio_resume();
}

thread_local! {
    static PENDING: RefCell<Vec<(u32, Job)>> = const { RefCell::new(Vec::new()) };
}

/// Queue work to run on the game thread a couple of frames from now.
pub fn defer(job: impl FnOnce() + 'static) {
    PENDING.with(|p| p.borrow_mut().push((0, Box::new(job))));
}

/// Called once per frame from the main loop: age the queue and run anything
/// that has waited long enough for its loading screen to be visible.
pub fn pump() {
    let ready: Vec<Job> = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        for (age, _) in p.iter_mut() {
            *age += 1;
        }
        let mut ready = Vec::new();
        let mut i = 0;
        while i < p.len() {
            if p[i].0 >= 2 {
                ready.push(p.remove(i).1);
            } else {
                i += 1;
            }
        }
        ready
    });
    if ready.is_empty() {
        return;
    }
    // A ready job is a synchronous song decode — hundreds of milliseconds that
    // block the event loop and, with it, the main-thread ScriptProcessorNode
    // that drives the demo's audio (web/kw_audio.js). A callback that can't
    // fire underruns, so the first decode of a session opens with a burst of
    // clicks. Suspend the context across the decode: the rendering thread stops
    // on its own thread and the gap is clean silence, then resumes. (Native
    // decodes on a worker thread, so its real-time audio never starves — this
    // is a browser-demo-only concern.)
    unsafe { kw_audio_suspend() };
    for job in ready {
        job();
    }
    unsafe { kw_audio_resume() };
}
