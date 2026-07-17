// Browser-demo support (wasm32 only). The demo ships one song —
// Code Monkey, fetched over HTTP as a .sng with vorbis stems — plus a locked
// "download to expand library" signpost row in the menu.

use std::cell::RefCell;
use std::sync::Arc;

use macroquad::experimental::coroutines::start_coroutine;
use macroquad::prelude::*;

use crate::chart::{self, SongEntry, SongSource};
use crate::gfx::draw_centered;
use crate::theme::{th, wa};

const DEMO_SONG_URL: &str = "songs/Code Monkey.sng";

/// Fetch the demo song and build the menu library, animating a download
/// screen while the multi-megabyte .sng arrives.
pub async fn load_demo_library() -> (Vec<SongEntry>, Vec<String>) {
    let fetch = start_coroutine(async { macroquad::file::load_file(DEMO_SONG_URL).await });
    loop {
        if let Some(res) = fetch.retrieve() {
            return match res {
                Ok(bytes) => demo_entries(bytes),
                Err(e) => (Vec::new(), vec![format!("demo song failed to download: {e}")]),
            };
        }
        clear_background(th().bg);
        draw_centered("KEYBOARD WARRIOR", 130.0, 72.0, Color::new(1.0, 1.0, 1.0, 0.95));
        draw_centered(
            "downloading demo song",
            screen_height() * 0.44,
            22.0,
            wa(th().secondary, 0.75),
        );
        // The same indeterminate sweep the song-loading scene uses
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
        next_frame().await;
    }
}

fn demo_entries(bytes: Vec<u8>) -> (Vec<SongEntry>, Vec<String>) {
    let source = SongSource::Bytes(Arc::new(bytes));
    match chart::load_song(&source) {
        Ok(chart) => {
            let available: Vec<usize> = (0..4).filter(|&d| chart.diffs[d].len() >= 20).collect();
            if available.is_empty() {
                return (Vec::new(), vec!["demo song: no difficulty with enough notes".into()]);
            }
            let song = SongEntry {
                title: if chart.title.is_empty() { "Code Monkey".into() } else { chart.title },
                artist: chart.artist,
                available,
                source,
                locked: false,
            };
            let signpost = SongEntry {
                title: "download to expand library".into(),
                artist: String::new(),
                available: Vec::new(),
                source: SongSource::Bytes(Arc::new(Vec::new())),
                locked: true,
            };
            (vec![song, signpost], Vec::new())
        }
        Err(e) => (Vec::new(), vec![format!("demo song: {e}")]),
    }
}

// ---------------------------------------------------------------- loader pump
//
// There are no threads on wasm, so song decoding runs on the game thread.
// Jobs are deferred by two frames so the loading screen has been drawn AND
// presented before the synchronous decode blocks the event loop; the frozen
// frame the player stares at is the loading screen, not the menu.

type Job = Box<dyn FnOnce()>;

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
    for job in ready {
        job();
    }
}
