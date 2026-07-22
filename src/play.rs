// The gameplay scene: notes, lanes, judging, scoring, and everything the
// highway draws.

use macroquad::prelude::*;

use crate::audio::{self, AudioEngine, Buf, Sounds};
use crate::chart::{Instrument, SongChart, DIFF_NAMES};
use crate::gfx::{draw_fit, dtext, msize, ui};
use crate::settings::{approach, sp_fx};
use crate::theme::{mix, th, wa};
use crate::words::{chart_seed, generate_text, text_mode, TextMode};

// Timing windows (seconds from the note's ideal time)
const PERFECT_WIN: f64 = 0.055;
const GREAT_WIN: f64 = 0.110;
const GOOD_WIN: f64 = 0.170;

// Negative feedback (miss sound + screen shake) is rate-limited: once it
// fires, it stays quiet for `MISS_FB_COOLDOWN` so a streak of misses doesn't
// turn into a wall of noise and thrashing. The miss still registers on screen
// (the floating "MISS"/"X", the lead ducking) — only the punchy sound and
// shake are gated. Purely cosmetic: scoring/judging is untouched.
const MISS_FB_COOLDOWN: f64 = 0.45;

// The lower-left gutter guitarist, benched until his art gets another pass.
// All of his logic still runs and draws when flipped back on.
const GUITARIST: bool = false;

#[derive(Clone, Copy, PartialEq)]
pub enum Judgement {
    Perfect,
    Great,
    Good,
}

impl Judgement {
    pub fn label(self) -> &'static str {
        match self {
            Judgement::Perfect => "PERFECT",
            Judgement::Great => "GREAT",
            Judgement::Good => "GOOD",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Judgement::Perfect => th().accent,
            Judgement::Great => th().secondary,
            Judgement::Good => th().good,
        }
    }
    pub fn score(self) -> i64 {
        match self {
            Judgement::Perfect => 300,
            Judgement::Great => 200,
            Judgement::Good => 100,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum NoteState {
    Pending,
    Hit(Judgement),
    Missed,
}

struct Note {
    ch: char,
    lane: usize,
    time: f64,    // song time in seconds when it should be typed
    sustain: f64, // hold length in seconds (0 = plain tap)
    word: usize,
    sp_phrase: Option<u16>, // star power phrase this note belongs to
    state: NoteState,
}

/// A sustain currently being held: bonus score accrues while the key stays
/// down, until the tail runs out or the finger lifts.
struct Hold {
    note: usize,
    key: char,    // the key actually pressed — in DFJK mode any key in the lane
    end: f64,     // timeline second the tail runs out
    partial: f32, // fractional bonus score carried between frames
}

pub fn lane_of(c: char) -> usize {
    match c {
        'q' | 'w' | 'a' | 's' | 'z' | 'x' => 0,
        'e' | 'r' | 't' | 'd' | 'f' | 'g' | 'c' | 'v' | 'b' => 1,
        'y' | 'u' | 'h' | 'j' | 'n' | 'm' => 2,
        // i/k/o/l/p plus the unshifted punctuation keys , . ' — right outer
        _ => 3,
    }
}

/// The lane a character rides in (gems) or aims at (keypresses). In DFJK
/// mode the four anchor keys pin to the four lanes — f, j, and k already
/// live in lanes 1–3, only d needs moving — and every other key keeps its
/// zone, so q still plays the D lane.
pub fn gem_lane(c: char) -> usize {
    if c == 'd' && text_mode() == TextMode::Dfjk {
        return 0;
    }
    lane_of(c)
}

/// Characters that can appear on gems: letters plus unshifted punctuation.
pub fn is_typeable(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, ',' | '.' | '\'')
}

/// The physical key that types a gem character.
fn key_of(c: char) -> Option<KeyCode> {
    Some(match c {
        'a' => KeyCode::A,
        'b' => KeyCode::B,
        'c' => KeyCode::C,
        'd' => KeyCode::D,
        'e' => KeyCode::E,
        'f' => KeyCode::F,
        'g' => KeyCode::G,
        'h' => KeyCode::H,
        'i' => KeyCode::I,
        'j' => KeyCode::J,
        'k' => KeyCode::K,
        'l' => KeyCode::L,
        'm' => KeyCode::M,
        'n' => KeyCode::N,
        'o' => KeyCode::O,
        'p' => KeyCode::P,
        'q' => KeyCode::Q,
        'r' => KeyCode::R,
        's' => KeyCode::S,
        't' => KeyCode::T,
        'u' => KeyCode::U,
        'v' => KeyCode::V,
        'w' => KeyCode::W,
        'x' => KeyCode::X,
        'y' => KeyCode::Y,
        'z' => KeyCode::Z,
        ',' => KeyCode::Comma,
        '.' => KeyCode::Period,
        '\'' => KeyCode::Apostrophe,
        _ => return None,
    })
}

/// Is the physical key for a gem character currently held down?
fn key_down(c: char) -> bool {
    key_of(c).is_some_and(is_key_down)
}

/// Was the character's key freshly pressed this frame? False for OS
/// key-repeat events from a key that is merely being held.
fn key_freshly_pressed(c: char) -> bool {
    key_of(c).is_some_and(is_key_pressed)
}

struct Particle {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max_life: f32,
    size: f32,
    color: Color,
}

struct Floater {
    text: String,
    pos: Vec2,
    life: f32,
    color: Color,
    size: f32,
}

// Which song was played, for the results screen and instant restarts
#[derive(Clone, Copy)]
pub struct SongRef {
    pub song: usize,            // index into the scanned song list
    pub diff: usize,            // chart difficulty
    pub instrument: Instrument, // which chart (guitar/bass) was played
}

struct SpPhrase {
    len: usize,
    hits: usize,
    broken: bool,
}

pub struct Play {
    pub song_ref: SongRef,
    pub title: String,
    pub diff_name: String,
    notes: Vec<Note>,
    words: Vec<String>,
    // First note that could still be Pending; hit/missed prefixes are never
    // rescanned, so keypress matching stays O(window) on long charts
    cursor: usize,
    // notes index where each word starts (notes are sorted, words contiguous)
    word_starts: Vec<usize>,
    holds: Vec<Hold>, // sustains currently being held
    whammying: bool,  // SHIFT is pressing the whammy bar on an active sustain
    whammy_vis: f32,  // eased bar position, drives the tail's bow on screen
    jam: f32,         // the gutter guitarist: 1 rocking out, 0 slumped still
    strum: f32,       // strum-hand flick, kicked to 1 on every clean hit
    pub paused: bool,
    pub pause_now: f64, // clock value frozen at the moment of pausing, for draw
    ducked: bool,       // lead stem is currently ducked after a miss
    beats: Vec<f64>,
    next_beat: usize,
    sp_phrases: Vec<SpPhrase>,
    energy: f32,
    sp_until: f64,
    sp_prev: bool,  // was SP active last update — releases the reverb on expiry
    sp_flash: f32,  // soft gold pulse over the highway at ignition
    spark_acc: f32, // fractional gold sparks carried between frames
    word_anim: f32, // eased index of the current word, drives the word queue
    spb: f64,
    pub score: i64,
    combo: i64,
    pub max_combo: i64,
    pub perfect: u32,
    pub great: u32,
    pub good: u32,
    pub miss: u32,
    pub strays: u32,
    particles: Vec<Particle>,
    floaters: Vec<Floater>,
    shake: f32,
    last_miss_fb: f64, // judge-clock time of the last miss/stray feedback, for the cooldown
    beat_flash: f32,
    first_note_time: f64,
    end_time: f64,
}

/// Gem radius as a fraction of the distance a note travels down the highway.
/// Taken from the reference layout (24 px gems over a 554 px run) and then
/// held fixed — see `Geom::radius`.
const GEM_PER_TRAVEL: f32 = 24.0 / 554.0;

struct Geom {
    left: f32,
    width: f32,
    lane_w: f32,
    hit_y: f32,
    top: f32,
    k: f32, // layout scale for the frame, cached so draw code can lean on it
}

fn geom() -> Geom {
    let w = screen_width();
    let h = screen_height();
    let k = ui();
    // The highway takes its share of a narrow window and grows with the scale
    // on a wide one, rather than freezing at a fixed width and leaving a big
    // display with a thin ribbon down the middle.
    let width = (w * 0.62).min(720.0 * k);
    let left = (w - width) / 2.0;
    // The strike line sits at a fixed fraction of the height so the word queue
    // below it always gets the same share. The top of the highway is then one
    // scaled travel distance above — which is what keeps note speed honest
    // across displays (see `radius`).
    let hit_y = h * 0.78;
    let top = (hit_y - 554.0 * k).max(h * 0.04);
    Geom { left, width, lane_w: width / 4.0, hit_y, top, k }
}

impl Geom {
    /// Distance a note covers from spawn to the strike line.
    fn travel(&self) -> f32 {
        self.hit_y - self.top
    }

    /// Gem radius, pinned to the travel distance instead of to the window.
    ///
    /// This is the one that matters for fairness. `approach()` is a constant
    /// *time*, so a taller window used to mean a longer run in the same two
    /// seconds — notes physically moved ~2.6x faster at 1440p than in a small
    /// window while the gems stayed 24 px either way, which quietly changed
    /// how hard the game was to read. Deriving the radius from the travel
    /// distance fixes the ratio: a note always covers the same number of its
    /// own diameters per second, on every display.
    ///
    /// Capped against the lane so gems can't grow into their neighbours on a
    /// short, very wide window.
    fn radius(&self) -> f32 {
        (self.travel() * GEM_PER_TRAVEL).min(self.lane_w * 0.30)
    }
}

/// Highway y for a timeline second: the strike line at `now`, the top of the
/// highway one approach-time later.
fn time_to_y(t: f64, g: &Geom, now: f64) -> f32 {
    g.hit_y - (((t - now) / approach()) as f32) * (g.hit_y - g.top)
}

/// The strike line, drawn as five segments with gaps at the lane centers so
/// it never cuts through the target rings or a gem crossing it.
fn draw_strike_line(g: &Geom, ox: f32, oy: f32, thickness: f32, color: Color) {
    let gap = g.radius() * 1.25;
    let y = g.hit_y + oy;
    let mut x = g.left + ox;
    for lane in 0..4 {
        let cx = g.left + g.lane_w * (lane as f32 + 0.5) + ox;
        if cx - gap > x {
            draw_line(x, y, cx - gap, y, thickness, color);
        }
        x = cx + gap;
    }
    let right = g.left + g.width + ox;
    if right > x {
        draw_line(x, y, right, y, thickness, color);
    }
}

/// RMS level of a stereo buffer in a ±50 ms window around `t` seconds.
fn rms_around(buf: &[[f32; 2]], rate: u32, t: f64) -> f32 {
    let a = ((t - 0.05) * rate as f64).max(0.0) as usize;
    let b = (((t + 0.05) * rate as f64) as usize).min(buf.len());
    if b <= a {
        return 0.0;
    }
    let sum: f32 = buf[a..b].iter().map(|s| s[0] * s[0] + s[1] * s[1]).sum();
    (sum / (b - a) as f32).sqrt()
}

/// Beat spacing of the tempo map at time `t`, in seconds per beat. Clamped
/// to 40–300 BPM so a degenerate map can't stretch the count-in absurdly.
fn beat_interval_at(beats: &[f64], t: f64) -> f64 {
    if beats.len() < 2 {
        return 0.5;
    }
    let i = beats.partition_point(|&b| b <= t).clamp(1, beats.len() - 1);
    (beats[i] - beats[i - 1]).clamp(0.2, 1.5)
}

/// The longest word a phrase can carry (the word pools go up to 8 letters).
const MAX_WORD: usize = 8;

/// Deal one run of notes (no internal rests) out as word-sized phrases.
/// A run that fits a word is one phrase. A longer run has to split, and a
/// blind cut at 8 can land mid-figure — Code Monkey's hard chart is triplet
/// cells (gaps 0.19 0.19 0.38 repeating), where straight 8s end every word
/// on the first note of the next cell. So each cut prefers the farthest
/// rhythmic seam — a gap noticeably wider than the run's tightest spacing —
/// within word range, and words end where the music breathes (that triplet
/// run deals 6s). A run with no seams (an even stream) or nothing but seams
/// still cuts at 8, so this only kicks in where the chart has real cells.
fn split_run(run: Vec<(f64, f64)>, out: &mut Vec<Vec<(f64, f64)>>) {
    if run.len() <= MAX_WORD {
        out.push(run);
        return;
    }
    let gaps: Vec<f64> = run.windows(2).map(|w| w[1].0 - w[0].0).collect();
    let base = gaps.iter().fold(f64::INFINITY, |a, &b| a.min(b)).max(0.05);
    // seam[i]: the gap after note i is wide enough to end a word at
    let seams: Vec<bool> = gaps.iter().map(|&g| g > base * 1.45).collect();
    let mut start = 0;
    while run.len() - start > MAX_WORD {
        let end = (start + 3..=start + MAX_WORD)
            .rev()
            .find(|&e| seams[e - 1])
            .unwrap_or(start + MAX_WORD);
        out.push(run[start..end].to_vec());
        start = end;
    }
    out.push(run[start..].to_vec());
}

/// Stream the text's letters onto note (time, sustain) pairs in order: letter
/// k of the text rides note k. Word boundaries drive the on-screen word queue.
fn assign_letters(words: &[String], times: &[(f64, f64)]) -> Vec<Note> {
    let mut notes = Vec::with_capacity(times.len());
    let (mut wi, mut li) = (0usize, 0usize);
    for &(t, len) in times.iter() {
        while wi < words.len() && li >= words[wi].len() {
            wi += 1;
            li = 0;
        }
        let Some(word) = words.get(wi) else { break };
        let ch = word.as_bytes()[li] as char;
        li += 1;
        notes.push(Note {
            ch,
            lane: gem_lane(ch),
            time: t,
            sustain: len,
            word: wi,
            sp_phrase: None,
            state: NoteState::Pending,
        });
    }
    notes
}

impl Play {
    /// Build a run from a Clone Hero chart: the charter's note timing becomes
    /// gems, grouped into phrases that carry real words. The stems are handed
    /// to the engine, which starts them at an exact frame after the count-in.
    pub fn new_chart(
        song_idx: usize,
        diff: usize,
        chart: &SongChart,
        engine: &AudioEngine,
        snd: &Sounds,
        backing: Buf,
        lead: Option<Buf>,
    ) -> Self {
        let times: Vec<(f64, f64)> = chart.diffs[diff].iter().map(|n| (n.time, n.len)).collect();

        // Group notes into runs at musical rests, then deal each run out as
        // word-sized phrases, cutting long runs at their rhythmic seams
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        for &(t, len) in &times {
            let new_run = match runs.last().and_then(|g| g.last()) {
                Some(&(prev, _)) => t - prev > 0.85,
                None => true,
            };
            if new_run {
                runs.push(Vec::new());
            }
            runs.last_mut().unwrap().push((t, len));
        }
        let mut groups: Vec<Vec<(f64, f64)>> = Vec::new();
        for run in runs {
            split_run(run, &mut groups);
        }
        // Fold lonely single-note groups into the previous word when close
        let mut merged: Vec<Vec<(f64, f64)>> = Vec::new();
        for g in groups {
            match merged.last_mut() {
                Some(prev)
                    if g.len() == 1
                        && prev.len() < MAX_WORD
                        && g[0].0 - prev.last().unwrap().0 < 1.6 =>
                {
                    prev.extend(g);
                }
                _ => merged.push(g),
            }
        }

        let group_lens: Vec<usize> = merged.iter().map(|g| g.len()).collect();
        let mut flat_times: Vec<(f64, f64)> = merged.concat();
        // Sustains: only tails long enough to be worth holding, clipped so
        // they never overlap the next note's press
        for i in 0..flat_times.len() {
            let next_t = flat_times.get(i + 1).map(|n| n.0);
            let (t, mut len) = flat_times[i];
            if let Some(nt) = next_t {
                len = len.min(nt - t - 0.12);
            }
            if len < 0.3 {
                len = 0.0;
            }
            flat_times[i].1 = len;
        }
        // WORDS (FIXED): pin the RNG to the chart so the same song and
        // difficulty always deal the same words, then hand the generator
        // back to the clock so nothing else replays between runs
        if text_mode() == TextMode::WordsFixed {
            macroquad::rand::srand(chart_seed(&chart.title, diff));
        }
        let words = generate_text(&group_lens);
        if text_mode() == TextMode::WordsFixed {
            macroquad::rand::srand(macroquad::miniquad::date::now() as u64);
        }
        let mut notes = assign_letters(&words, &flat_times);

        // Star power: tag notes inside each SP span and record phrase sizes
        let mut sp_phrases = Vec::new();
        for &(s, e) in &chart.sp[diff] {
            let members: Vec<usize> = notes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.time >= s - 1e-6 && n.time < e)
                .map(|(i, _)| i)
                .collect();
            if members.len() >= 2 {
                let id = sp_phrases.len() as u16;
                for &i in &members {
                    notes[i].sp_phrase = Some(id);
                }
                sp_phrases.push(SpPhrase { len: members.len(), hits: 0, broken: false });
            }
        }

        let first = notes.first().map_or(0.0, |n| n.time);
        // Beat interval measured AT the first note — charts can change tempo
        // (or open with a placeholder bar) before the notes start, so
        // beats[1]-beats[0] can count at the wrong speed
        let spb = beat_interval_at(&chart.beats, first);
        let end_time = chart.end + 3.0;
        // The stems begin at exactly timeline zero. Count-in: four hi-hat
        // ticks on the real beat grid walking into the FIRST NOTE — matching
        // the on-screen countdown. (Counting into timeline zero is useless:
        // charts pad seconds of empty bars before the notes.) The grid
        // extends backward, and the lead-in stretches, when a chart opens
        // immediately.
        let bi = chart.beats.partition_point(|&b| b < first - 1e-6);
        let ticks: Vec<f64> = (1..=4usize)
            .map(|k| bi.checked_sub(k).map_or(first - k as f64 * spb, |j| chart.beats[j]))
            .collect();
        // ...but only when the recording is quiet under them. Plenty of rips
        // open with their own stick count or a musical intro; a synthesized
        // click on top plays flams against the one or fights the other, so
        // there the recording itself is the count. "Most quiet", not "all":
        // the last tick often brushes the swell of the music coming in.
        let quiet_at = |t: f64| {
            let mut r = rms_around(&backing, engine.sample_rate, t);
            if let Some(l) = &lead {
                r += rms_around(l, engine.sample_rate, t);
            }
            r < 0.05
        };
        let count_in = ticks.iter().filter(|&&t| quiet_at(t)).count() >= 3;
        let earliest = ticks.last().copied().unwrap_or(0.0);
        let lead_in = if count_in { (0.4 - earliest).max(3.0) } else { 3.0 };
        engine.start_timeline(lead_in, Some(backing), lead);
        if count_in {
            for &t in &ticks {
                engine.play_at(&snd.hat, 0.8, t);
            }
        }
        Self::from_parts(
            SongRef { song: song_idx, diff, instrument: chart.instrument },
            chart.title.clone(),
            DIFF_NAMES[diff].to_string(),
            notes,
            words,
            chart.beats.clone(),
            sp_phrases,
            spb,
            first,
            end_time,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        song_ref: SongRef,
        title: String,
        diff_name: String,
        notes: Vec<Note>,
        words: Vec<String>,
        beats: Vec<f64>,
        sp_phrases: Vec<SpPhrase>,
        spb: f64,
        first_note_time: f64,
        end_time: f64,
    ) -> Self {
        let mut word_starts = vec![notes.len(); words.len()];
        for (i, n) in notes.iter().enumerate().rev() {
            word_starts[n.word] = i;
        }
        Play {
            song_ref,
            title,
            diff_name,
            notes,
            words,
            cursor: 0,
            word_starts,
            holds: Vec::new(),
            whammying: false,
            whammy_vis: 0.0,
            jam: 1.0,
            strum: 0.0,
            paused: false,
            pause_now: 0.0,
            ducked: false,
            beats,
            next_beat: 0,
            sp_phrases,
            energy: 0.0,
            // NEG_INFINITY, not -1: the clock is negative during the
            // count-in, and `now < sp_until` must not read as active there
            sp_until: f64::NEG_INFINITY,
            sp_prev: false,
            sp_flash: 0.0,
            spark_acc: 0.0,
            word_anim: 0.0,
            spb,
            score: 0,
            combo: 0,
            max_combo: 0,
            perfect: 0,
            great: 0,
            good: 0,
            miss: 0,
            strays: 0,
            particles: Vec::new(),
            floaters: Vec::new(),
            shake: 0.0,
            last_miss_fb: f64::NEG_INFINITY,
            beat_flash: 0.0,
            first_note_time,
            end_time,
        }
    }

    fn sp_active(&self, now: f64) -> bool {
        now < self.sp_until
    }

    /// Whether the punchy miss/stray feedback (sound + shake) should fire at
    /// judge-clock time `at`. Returns true and re-arms the cooldown only if at
    /// least `MISS_FB_COOLDOWN` has passed since it last fired, so a run of
    /// misses gets one thump rather than a stream of them.
    fn miss_feedback(&mut self, at: f64) -> bool {
        if at - self.last_miss_fb < MISS_FB_COOLDOWN {
            return false;
        }
        self.last_miss_fb = at;
        true
    }

    /// Skip the cursor past notes that can never change state again.
    fn advance_cursor(&mut self) {
        while self.cursor < self.notes.len() && self.notes[self.cursor].state != NoteState::Pending
        {
            self.cursor += 1;
        }
    }

    fn multiplier(&self, now: f64) -> i64 {
        let base = 1 + (self.combo / 10).min(3);
        if self.sp_active(now) {
            base * 2
        } else {
            base
        }
    }

    fn note_pos(&self, note: &Note, g: &Geom, now: f64) -> Vec2 {
        vec2(g.left + g.lane_w * (note.lane as f32 + 0.5), time_to_y(note.time, g, now))
    }

    /// Effects are authored at reference scale and multiplied in here, so the
    /// callers below stay readable — a burst throws the same-looking spray
    /// whatever the window size. (Velocities are baked at spawn; a resize
    /// mid-flight only affects particles already in the air, which live well
    /// under a second.)
    fn burst(&mut self, pos: Vec2, color: Color, count: usize) {
        let k = ui();
        for _ in 0..count {
            let ang = macroquad::rand::gen_range(0.0f32, std::f32::consts::TAU);
            let speed = macroquad::rand::gen_range(60.0f32, 380.0) * k;
            let life = macroquad::rand::gen_range(0.25f32, 0.6);
            self.particles.push(Particle {
                pos,
                vel: vec2(ang.cos(), ang.sin()) * speed,
                life,
                max_life: life,
                size: macroquad::rand::gen_range(2.0f32, 5.5) * k,
                color,
            });
        }
    }

    fn float_text(&mut self, text: &str, pos: Vec2, color: Color, size: f32) {
        let size = size * ui();
        self.floaters.push(Floater { text: text.to_string(), pos, life: 0.8, color, size });
    }

    pub fn handle_char(&mut self, c: char, now: f64, snd: &Sounds, engine: &AudioEngine) {
        // Space deploys banked star power: 2x score while it lasts
        if c == ' ' {
            if self.energy >= 0.5 && !self.sp_active(now) {
                self.sp_until = now + self.energy as f64 * 16.0;
                self.energy = 0.0;
                let g = geom();
                let cx = g.left + g.width / 2.0;
                self.float_text(
                    "STAR POWER!",
                    vec2(cx, g.hit_y - 130.0),
                    wa(th().accent, 1.0),
                    44.0,
                );
                // The ignition stays understated: one soft gold pulse over
                // the highway, a quiet thump, and a touch of hall behind
                // the lead — the drifting sparks carry it from there.
                // All of it sits behind the settings toggle; the scoring
                // and the gold state indicators don't.
                if sp_fx() {
                    self.sp_flash = 1.0;
                    engine.play(&snd.sp_start, 0.8);
                    engine.set_sp_fx(1.0);
                }
            }
            return;
        }
        // macOS smart-punctuation text input turns a typed ' into a curly
        // quote (U+2018/2019) before it ever reaches us — normalize back to
        // the ascii apostrophe gems actually use, or the note just blows by
        let c = match c.to_ascii_lowercase() {
            '\u{2018}' | '\u{2019}' => '\'',
            c => c,
        };
        if !is_typeable(c) {
            return;
        }
        if now < self.first_note_time - GOOD_WIN {
            return; // still in the count-in
        }
        // OS key-repeat from a held key (a sustain, or just a lingering
        // finger) is not a fresh press: it never judges and never strays
        if !key_freshly_pressed(c) {
            return;
        }
        let g = geom();

        // DFJK mode judges by lane, not letter: any key aimed at the gem's
        // lane counts, so q hits the D lane, m hits the J lane, and so on
        let by_lane = text_mode() == TextMode::Dfjk;
        self.advance_cursor();
        let mut best: Option<(usize, f64)> = None;
        for i in self.cursor..self.notes.len() {
            let n = &self.notes[i];
            if n.time - now > GOOD_WIN {
                break; // notes are sorted by time
            }
            let matches = if by_lane { n.lane == gem_lane(c) } else { n.ch == c };
            if n.state != NoteState::Pending || !matches {
                continue;
            }
            let dt = now - n.time;
            if dt.abs() <= GOOD_WIN && best.is_none_or(|(_, b)| dt.abs() < b.abs()) {
                best = Some((i, dt));
            }
        }

        match best {
            Some((i, dt)) => {
                let j = if dt.abs() <= PERFECT_WIN {
                    Judgement::Perfect
                } else if dt.abs() <= GREAT_WIN {
                    Judgement::Great
                } else {
                    Judgement::Good
                };
                self.notes[i].state = NoteState::Hit(j);
                self.combo += 1;
                self.max_combo = self.max_combo.max(self.combo);
                self.score += j.score() * self.multiplier(now);
                match j {
                    Judgement::Perfect => self.perfect += 1,
                    Judgement::Great => self.great += 1,
                    Judgement::Good => self.good += 1,
                }
                // Star power phrase progress: complete a phrase cleanly —
                // every note hit, none missed — to bank energy. Completing
                // one while the power is already burning feeds the flame
                // instead: the same quarter-bar lands as extra seconds.
                if let Some(p) = self.notes[i].sp_phrase {
                    let ph = &mut self.sp_phrases[p as usize];
                    ph.hits += 1;
                    if !ph.broken && ph.hits == ph.len {
                        if self.sp_active(now) {
                            self.sp_until = (self.sp_until + 0.25 * 16.0).min(now + 16.0);
                        } else {
                            self.energy = (self.energy + 0.25).min(1.0);
                        }
                        let g2 = geom();
                        self.float_text(
                            "STAR POWER +",
                            vec2(g2.left + g2.width / 2.0, g2.hit_y - 100.0 * g2.k),
                            wa(th().accent, 1.0),
                            30.0,
                        );
                    }
                }
                let pos = {
                    let n = &self.notes[i];
                    self.note_pos(n, &g, now)
                };
                let lane_color = th().lane[self.notes[i].lane];
                let count = if j == Judgement::Perfect { 18 } else { 10 };
                self.burst(pos, lane_color, count);
                if j == Judgement::Perfect {
                    self.burst(pos, WHITE, 6);
                }
                self.float_text(j.label(), vec2(pos.x, g.hit_y - 64.0 * g.k), j.color(), 26.0);
                // A sustained gem starts a hold: keep the key down for bonus
                if self.notes[i].sustain > 0.0 {
                    let n = &self.notes[i];
                    self.holds.push(Hold {
                        note: i,
                        key: c,
                        end: n.time + n.sustain,
                        partial: 0.0,
                    });
                }
                // A clean hit brings the ducked lead stem back into the mix
                // and puts the gutter guitarist back to work
                self.strum = 1.0;
                if self.ducked {
                    engine.set_lead_gain(1.0);
                    self.ducked = false;
                }
            }
            None => {
                // Stray keypress: no matching note in the window. The
                // guitarist stumbles but recovers on his own
                self.strays += 1;
                self.combo = 0;
                self.jam = self.jam.min(0.35);
                let xpos = vec2(g.left + g.width / 2.0, g.hit_y - 40.0 * g.k);
                self.float_text("X", xpos, th().miss, 24.0);
                if self.miss_feedback(now) {
                    self.shake = self.shake.max(3.0 * g.k);
                    engine.play(&snd.miss, 0.18);
                }
            }
        }
    }

    /// `jnow` is the judged clock (raw audio clock minus the calibration
    /// offset). It drives everything the player sees and is judged against, so
    /// the highway lines up with the strike line at the perfect-press moment.
    pub fn update(&mut self, jnow: f64, snd: &Sounds, engine: &AudioEngine) {
        let dt = get_frame_time();
        let g = geom();

        // Visual pulse on each beat (beat times come from the tempo map)
        while self.next_beat < self.beats.len() && self.beats[self.next_beat] <= jnow {
            self.beat_flash = 1.0;
            self.next_beat += 1;
        }

        // Notes that sailed past the window become misses
        self.advance_cursor();
        let mut missed = Vec::new();
        for i in self.cursor..self.notes.len() {
            if self.notes[i].time >= jnow - GOOD_WIN {
                break; // notes are sorted by time
            }
            if self.notes[i].state == NoteState::Pending {
                self.notes[i].state = NoteState::Missed;
                missed.push(i);
            }
        }
        for i in missed {
            self.miss += 1;
            self.combo = 0;
            if self.miss_feedback(jnow) {
                self.shake = self.shake.max(7.0 * g.k);
                engine.play(&snd.miss, 0.35);
            }
            if let Some(p) = self.notes[i].sp_phrase {
                self.sp_phrases[p as usize].broken = true;
            }
            let x = g.left + g.lane_w * (self.notes[i].lane as f32 + 0.5);
            self.float_text("MISS", vec2(x, g.hit_y - 64.0 * g.k), wa(th().miss, 1.0), 26.0);
            // Fumbling the line makes the lead drop out of the mix
            if !self.ducked {
                engine.set_lead_gain(0.12);
                self.ducked = true;
            }
        }

        // Sustain holds: bonus score drips in while the key stays down.
        // Lifting early just stops the bonus — no combo break, like GH.
        // SHIFT is the whammy bar: holding it keeps the lead bent down and
        // fattened, releasing returns it to normal. While pressed on a
        // sustain it also doubles the drip and trickles star power.
        // (No whammy bar in the browser demo — real app only.)
        let shift = !cfg!(target_arch = "wasm32")
            && (is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
        let mult = self.multiplier(jnow) as f32 * if shift { 2.0 } else { 1.0 };
        let mut holds = std::mem::take(&mut self.holds);
        let mut bonus = 0i64;
        let mut done: Vec<usize> = Vec::new();
        holds.retain_mut(|h| {
            if jnow >= h.end {
                done.push(h.note);
                return false;
            }
            if !key_down(h.key) {
                return false;
            }
            h.partial += dt * 60.0 * mult;
            let whole = h.partial.floor();
            h.partial -= whole;
            bonus += whole as i64;
            true
        });
        self.score += bonus;
        for i in done {
            let x = g.left + g.lane_w * (self.notes[i].lane as f32 + 0.5);
            self.burst(vec2(x, g.hit_y), th().lane[self.notes[i].lane], 8);
        }
        self.holds = holds;
        let whammy = shift && !self.holds.is_empty();
        if whammy {
            self.energy = (self.energy + dt * 0.05).min(1.0);
        }
        if whammy != self.whammying {
            self.whammying = whammy;
            engine.set_whammy(if whammy { 1.0 } else { 0.0 });
        }
        // Eased bar position for the tail's bow, mirroring the audio ramp
        self.whammy_vis += ((whammy as i32 as f32) - self.whammy_vis) * (1.0 - (-dt * 13.0).exp());

        // Star power ambience: gold flecks drift up the screen while it
        // burns; the moment it dies the lead's reverb send is released
        // (its tail rings out in the mixer)
        let sp_on = self.sp_active(jnow);
        if self.sp_prev && !sp_on {
            engine.set_sp_fx(0.0);
        }
        self.sp_prev = sp_on;
        if sp_on && sp_fx() {
            // Flecks cover the whole screen, gutters included; the spawn rate
            // scales with the extra area so the highway doesn't look thinner
            let sw = screen_width();
            self.spark_acc += dt * 26.0 * (sw / g.width).max(1.0);
            while self.spark_acc >= 1.0 {
                self.spark_acc -= 1.0;
                let x = macroquad::rand::gen_range(0.0, sw);
                let y = macroquad::rand::gen_range(g.top, g.hit_y);
                let life = macroquad::rand::gen_range(0.35f32, 0.8);
                self.particles.push(Particle {
                    pos: vec2(x, y),
                    vel: vec2(
                        macroquad::rand::gen_range(-18.0f32, 18.0) * g.k,
                        -macroquad::rand::gen_range(60.0f32, 170.0) * g.k,
                    ),
                    life,
                    max_life: life,
                    size: macroquad::rand::gen_range(1.5f32, 3.2) * g.k,
                    color: mix(th().accent, WHITE, macroquad::rand::gen_range(0.0f32, 0.5)),
                });
            }
        }
        self.sp_flash = (self.sp_flash - 2.0 * dt).max(0.0);

        // The guitarist follows the lead stem: while it's ducked after a
        // miss he stands slumped; the next clean hit winds him back up
        let jam_t = if self.ducked { 0.0 } else { 1.0 };
        self.jam += (jam_t - self.jam) * (1.0 - (-dt * 6.0).exp());
        self.strum = (self.strum - dt * 5.0).max(0.0);

        // Effects. Anything measured in pixels per second scales, so arcs and
        // drifts keep their shape rather than flattening out on a big display.
        let k = ui();
        for p in self.particles.iter_mut() {
            p.pos += p.vel * dt;
            p.vel *= 1.0 - 2.5 * dt;
            p.vel.y += 300.0 * k * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for f in self.floaters.iter_mut() {
            f.pos.y -= 55.0 * k * dt;
            f.life -= dt;
        }
        self.floaters.retain(|f| f.life > 0.0);
        self.shake = (self.shake - 30.0 * k * dt).max(0.0);
        self.beat_flash = (self.beat_flash - 4.0 * dt).max(0.0);

        // Ease the word queue toward the current word (completed words slide
        // up and out, upcoming words rise into place). After advance_cursor
        // the cursor sits on the first pending note.
        self.advance_cursor();
        let target = self.notes.get(self.cursor).map(|n| n.word).unwrap_or(self.words.len()) as f32;
        let k = 1.0 - (-dt * 9.0).exp();
        self.word_anim += (target - self.word_anim) * k;
    }

    pub fn finished(&self, now: f64) -> bool {
        now > self.end_time + 1.0
    }

    fn hits(&self) -> u32 {
        self.perfect + self.great + self.good
    }

    pub fn accuracy(&self) -> f64 {
        let total = self.hits() + self.miss + self.strays;
        if total == 0 {
            return 100.0;
        }
        self.hits() as f64 / total as f64 * 100.0
    }

    // ------------------------------------------------------------ rendering

    /// The gutter guitarist: a stylized silhouette in the lower-left, built
    /// from the same primitives as the board. While the lead is in the mix
    /// he rocks — bobbing on the beat, near foot tapping, strum hand
    /// flicking on every clean hit. A miss ducks the lead and he slumps to
    /// a standstill, guitar neck drooping, until the next hit winds him up.
    ///
    /// Benched behind GUITARIST for now — the pose reads but needs art
    /// passes before he earns the stage. The jam/strum state keeps updating
    /// either way, so switching him back on is a one-flag change.
    fn draw_guitarist(&self, now: f64) {
        if !GUITARIST {
            return;
        }
        let g = geom();
        let k = g.k;
        if g.left < 140.0 * k {
            return; // window too narrow — no gutter to stand in
        }
        let h = screen_height();
        // Sized off how roomy the gutter is relative to the layout scale, then
        // scaled, so he fills his corner the same way at any window size
        let s = (g.left / (200.0 * k)).clamp(0.8, 1.3) * k;
        let cx = g.left * 0.5;
        let base = h - 34.0 * k;
        let jam = self.jam;
        let slump = 1.0 - jam;
        let beat = (now / self.spb.max(0.2)) as f32 * std::f32::consts::TAU;
        let bob = beat.sin() * 4.0 * jam * s;
        let a = wa(WHITE, 0.20 + 0.18 * jam);
        let acc = th().accent;

        // Star power puts him under a spotlight
        if self.sp_active(now) {
            draw_circle(cx, base - 60.0 * s, 85.0 * s, wa(acc, 0.05));
        }
        draw_ellipse(cx, base + 6.0 * s, 44.0 * s, 7.0 * s, 0.0, Color::new(0.0, 0.0, 0.0, 0.35));

        let hip = vec2(cx - 2.0 * s, base - 56.0 * s + bob * 0.4);
        let sh = vec2(hip.x + 3.0 * s - 7.0 * s * slump, hip.y - 42.0 * s + bob + 7.0 * s * slump);
        // Legs — the near foot taps while he's going
        let tap = beat.sin().max(0.0) * 7.0 * jam * s;
        draw_line(hip.x, hip.y, cx - 19.0 * s, base - tap, 6.5 * s, a);
        draw_line(hip.x, hip.y, cx + 15.0 * s, base, 6.5 * s, a);
        draw_line(sh.x, sh.y, hip.x, hip.y, 10.0 * s, a);
        // Head nods a half-beat off the body bob, hangs when slumped
        let head = vec2(
            sh.x + 3.0 * s - 3.0 * s * slump,
            sh.y - 17.0 * s + (beat + 1.2).sin() * 3.0 * jam * s + 5.0 * s * slump,
        );
        draw_circle(head.x, head.y, 10.5 * s, a);

        // Guitar: double-bout body on a neck that points up-left and droops
        // with him
        let gc = vec2(hip.x + 12.0 * s, hip.y - 8.0 * s + bob * 0.5);
        let dir = vec2(-1.0, -0.55 + 0.75 * slump).normalize();
        let neck_end = gc + dir * 58.0 * s;
        draw_line(gc.x, gc.y, neck_end.x, neck_end.y, 4.0 * s, wa(WHITE, 0.30 + 0.20 * jam));
        let hs = neck_end + dir * 9.0 * s;
        draw_line(neck_end.x, neck_end.y, hs.x, hs.y, 6.5 * s, a);
        draw_circle(gc.x - 7.0 * s, gc.y + 1.0 * s, 14.0 * s, mix(th().bg, acc, 0.30));
        draw_circle(gc.x + 6.0 * s, gc.y + 2.0 * s, 11.5 * s, mix(th().bg, acc, 0.30));
        draw_circle_lines(
            gc.x - 2.0 * s,
            gc.y + 1.5 * s,
            13.5 * s,
            2.0 * s,
            wa(acc, 0.45 + 0.40 * jam),
        );
        draw_circle(gc.x - 2.0 * s, gc.y + 1.0 * s, 3.5 * s, wa(th().bg, 0.9));

        // Fret hand rides the neck; strum arm swings at the body, snapping
        // with each hit, hanging loose when he stops
        let fret = neck_end - dir * 12.0 * s;
        draw_line(sh.x, sh.y, fret.x, fret.y, 5.5 * s, a);
        draw_circle(fret.x, fret.y, 4.0 * s, a);
        let swing = beat.sin() * 0.45 * jam + self.strum * 0.55;
        let ang = 1.15 - swing;
        let jam_hand = gc + vec2(ang.cos(), ang.sin()) * 21.0 * s;
        let hang = vec2(sh.x + 6.0 * s, sh.y + 40.0 * s);
        let hand = hang.lerp(jam_hand, jam);
        let elbow = vec2(sh.x + 16.0 * s, sh.y + 16.0 * s);
        draw_line(sh.x, sh.y, elbow.x, elbow.y, 5.5 * s, a);
        draw_line(elbow.x, elbow.y, hand.x, hand.y, 5.5 * s, a);
        draw_circle(hand.x, hand.y, 4.0 * s, a);
    }

    /// `now` here is the judged clock (calibration offset already applied), so
    /// the highway and strike line agree with where notes are judged.
    pub fn draw(&self, now: f64) {
        let g = geom();
        let h = screen_height();
        let ap = approach();
        let k = g.k;
        let radius = g.radius();
        // How far past either edge a gem still counts as worth drawing
        let cull = radius * 2.5;

        let (ox, oy) = if self.shake > 0.0 {
            (
                macroquad::rand::gen_range(-self.shake, self.shake),
                macroquad::rand::gen_range(-self.shake, self.shake),
            )
        } else {
            (0.0, 0.0)
        };

        // Background wash that breathes with the beat
        clear_background(th().bg);

        // Highway backdrop
        draw_rectangle(g.left + ox, 0.0, g.width, h, Color::new(1.0, 1.0, 1.0, 0.03));
        for i in 0..=4 {
            let x = g.left + g.lane_w * i as f32 + ox;
            draw_line(x, 0.0, x, h, 1.0, Color::new(1.0, 1.0, 1.0, 0.10));
        }

        // Scrolling beat grid
        let travel = g.hit_y - g.top;
        let lo = self.beats.partition_point(|&b| b < now);
        for bi in lo..self.beats.len() {
            let t = self.beats[bi];
            if t > now + ap {
                break;
            }
            let progress = ((t - now) / ap) as f32;
            let y = g.hit_y - progress * travel + oy;
            let alpha = if bi % 4 == 0 { 0.14 } else { 0.05 };
            draw_line(
                g.left + ox,
                y,
                g.left + g.width + ox,
                y,
                1.0,
                Color::new(1.0, 1.0, 1.0, alpha),
            );
        }

        // Strike line, drawn in segments that skip the lane circles so it
        // never runs through a gem or target ring
        let flash = 0.42 + 0.14 * self.beat_flash;
        draw_strike_line(&g, ox, oy, 4.0 * k, Color::new(1.0, 1.0, 1.0, flash));
        for lane in 0..4 {
            let x = g.left + g.lane_w * (lane as f32 + 0.5) + ox;
            let mut c = th().lane[lane];
            c.a = 0.30 + 0.12 * self.beat_flash;
            // Target rings match the gems that land in them
            draw_circle_lines(x, g.hit_y + oy, radius, 2.0 * k, c);
        }

        // Only notes near the screen need drawing: anything more than one
        // approach-window old is far below it. Ahead, the window is stretched
        // past the highway top so gems spawn above the window edge and drift
        // in instead of popping in at the top. Notes are sorted: a slice.
        let spawn_ap = ap * ((g.hit_y + cull) / g.travel()) as f64;
        let visible_lo = self.notes.partition_point(|n| n.time < now - ap);

        // Connectors between letters of the same word
        for w in self.notes[visible_lo.saturating_sub(1)..].windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if a.time - now > spawn_ap {
                break;
            }
            if a.word != b.word || a.state != NoteState::Pending || b.state != NoteState::Pending {
                continue;
            }
            let pa = self.note_pos(a, &g, now) + vec2(ox, oy);
            let pb = self.note_pos(b, &g, now) + vec2(ox, oy);
            if pa.y < -cull && pb.y < -cull {
                continue;
            }
            draw_line(pa.x, pa.y, pb.x, pb.y, 2.0 * k, Color::new(1.0, 1.0, 1.0, 0.13));
        }

        // Sustain tails run from each gem up toward its release point; drawn
        // under the gems so the gem caps the tail's base
        for n in &self.notes[visible_lo..] {
            if n.time - now > spawn_ap {
                break;
            }
            if n.sustain <= 0.0 || n.state != NoteState::Pending {
                continue;
            }
            let pos = self.note_pos(n, &g, now) + vec2(ox, oy);
            if pos.y < -cull || pos.y > h + cull {
                continue;
            }
            let y_end = (time_to_y(n.time + n.sustain, &g, now) + oy).max(-20.0 * k);
            draw_line(pos.x, pos.y, pos.x, y_end, 5.0 * k, wa(th().lane[n.lane], 0.22));
        }

        // Gems
        for n in &self.notes[visible_lo..] {
            if n.time - now > spawn_ap {
                break;
            }
            let pos = self.note_pos(n, &g, now) + vec2(ox, oy);
            if pos.y < -cull || pos.y > h + cull {
                continue;
            }
            // Letter size follows the gem, so a gem always reads as a gem
            let label_size = radius * 1.25;
            match n.state {
                NoteState::Pending => {
                    // Dark-bodied gem with a lane-colored ring and letter —
                    // reads as part of the theme instead of a solid disc
                    let closeness = (1.0 - ((n.time - now) / ap).clamp(0.0, 1.0) as f32).powi(2);
                    let lane_c = th().lane[n.lane];
                    let mut glow = lane_c;
                    glow.a = 0.08 + 0.20 * closeness;
                    draw_circle(pos.x, pos.y, radius * (1.25 + 0.17 * closeness), glow);
                    draw_circle(pos.x, pos.y, radius, mix(th().bg, lane_c, 0.16));
                    // Gold styling only while the chain is still alive: a
                    // missed note breaks the phrase and its remaining gems
                    // fall back to plain lane colors
                    let sp_live = n.sp_phrase.is_some_and(|p| !self.sp_phrases[p as usize].broken);
                    let ring = if sp_live { th().accent } else { lane_c };
                    draw_circle_lines(
                        pos.x,
                        pos.y,
                        radius,
                        2.5 * k,
                        wa(ring, 0.75 + 0.25 * closeness),
                    );
                    if sp_live {
                        draw_circle_lines(
                            pos.x,
                            pos.y,
                            radius * 1.17,
                            1.5 * k,
                            wa(th().accent, 0.45),
                        );
                    }
                    let label = n.ch.to_ascii_uppercase().to_string();
                    let dims = msize(&label, label_size);
                    dtext(
                        &label,
                        pos.x - dims.width / 2.0,
                        pos.y + dims.height / 2.0,
                        label_size,
                        mix(lane_c, WHITE, 0.25),
                    );
                }
                NoteState::Missed => {
                    draw_circle(pos.x, pos.y, radius, mix(th().bg, th().miss, 0.12));
                    draw_circle_lines(pos.x, pos.y, radius, 2.0 * k, wa(th().miss, 0.4));
                    let label = n.ch.to_ascii_uppercase().to_string();
                    let dims = msize(&label, label_size);
                    dtext(
                        &label,
                        pos.x - dims.width / 2.0,
                        pos.y + dims.height / 2.0,
                        label_size,
                        wa(th().miss, 0.45),
                    );
                }
                NoteState::Hit(_) => {}
            }
        }

        // Active holds: the remaining tail drains into a glowing anchor on
        // the strike line while the key stays down. Pressing the whammy bar
        // fattens the tail and sends a wave rippling down it — pulsing in
        // time with the audio's pitch pump — releasing lets it slim and
        // straighten again, GH-style.
        for hd in &self.holds {
            let n = &self.notes[hd.note];
            let x = g.left + g.lane_w * (n.lane as f32 + 0.5) + ox;
            let y_end = (time_to_y(n.time + n.sustain, &g, now) + oy).max(-20.0 * k);
            let c = th().lane[n.lane];
            let vis = self.whammy_vis;
            // Same pump rate as the audio LFO, so width and pitch breathe as one
            let pump_ph = now * std::f64::consts::TAU * audio::WH_PUMP_HZ;
            let pump = (0.5 - 0.5 * pump_ph.cos()) as f32;
            let thick = (7.0 + (6.0 + 2.5 * pump) * vis) * k;
            if vis > 0.02 {
                let anchor = g.hit_y + oy;
                let wave_t = pump_ph as f32;
                let step = 9.0 * k;
                let mut prev = vec2(x, anchor);
                let mut yy = anchor - step;
                loop {
                    let seg_y = yy.max(y_end);
                    let d = anchor - seg_y; // distance up the tail, px
                                            // Traveling wave, pinned at the anchor so the base stays
                                            // planted on the strike line. Wavelength and ramp-in are
                                            // scaled too, so the tail keeps its shape at any size
                                            // instead of turning into a tight ripple when zoomed up.
                    let amp = 6.5 * k * vis * (d / (60.0 * k)).min(1.0);
                    let p = vec2(x + (d * 0.055 / k + wave_t).sin() * amp, seg_y);
                    // Soft halo under the core line doubles the tail's body
                    draw_line(prev.x, prev.y, p.x, p.y, thick + 8.0 * k, wa(c, 0.22 * vis));
                    draw_line(prev.x, prev.y, p.x, p.y, thick, wa(c, 0.78));
                    if seg_y <= y_end {
                        break;
                    }
                    prev = p;
                    yy -= step;
                }
            } else {
                draw_line(x, g.hit_y + oy, x, y_end, thick, wa(c, 0.75));
            }
            let pump_r = (3.0 + 2.0 * pump) * vis * k;
            draw_circle(x, g.hit_y + oy, 12.0 * k + pump_r, wa(c, 0.9));
            draw_circle_lines(
                x,
                g.hit_y + oy,
                (19.0 + 3.0 * self.beat_flash) * k,
                2.0 * k,
                wa(c, 0.6),
            );
        }

        // Particles & floaters
        for p in &self.particles {
            let mut c = p.color;
            c.a = (p.life / p.max_life).clamp(0.0, 1.0);
            draw_circle(p.pos.x + ox, p.pos.y + oy, p.size * (p.life / p.max_life), c);
        }
        for f in &self.floaters {
            let mut c = f.color;
            c.a = (f.life / 0.8).clamp(0.0, 1.0);
            let dims = msize(&f.text, f.size);
            dtext(&f.text, f.pos.x - dims.width / 2.0 + ox, f.pos.y + oy, f.size, c);
        }

        self.draw_guitarist(now);

        // Word queue below the strike line: the current word large with live
        // per-letter results, upcoming words stacked beneath it smaller and
        // dimmer, everything easing upward as words complete. The next letter
        // to type is subtly larger and underlined so a lost eye can re-anchor.
        let next_letter: Option<(usize, usize)> = self.notes[self.cursor..]
            .iter()
            .position(|n| n.state == NoteState::Pending)
            .map(|off| {
                let i = self.cursor + off;
                let w = self.notes[i].word;
                (w, i - self.word_starts[w])
            });
        // The queue lives between the strike line and the bottom edge. That
        // band is a fixed share of the height, so rather than let the far rows
        // drop off a short window — which costs the player their read-ahead,
        // and used to start happening just under the default window size — the
        // row pitch compresses to whatever room there is. The full lookahead
        // is always on screen; it just sits tighter when there's less room.
        const QUEUE_ROWS: f32 = 3.6;
        let queue_top = g.hit_y + 84.0 * k;
        let room = (h - 6.0 * k) - queue_top;
        // Floored: on a window short enough that `room` goes negative the
        // rows would otherwise march back up into the highway. They spill off
        // the bottom instead, which the cull below already handles.
        let row_h = (25.0 * k).min(room / QUEUE_ROWS).max(8.0 * k);
        let first_row = (self.word_anim.floor().max(0.0)) as usize;
        for wi in first_row..self.words.len() {
            let offset = wi as f32 - self.word_anim;
            if offset > QUEUE_ROWS {
                break;
            }
            let y = queue_top + offset * row_h + oy;
            if y > h - 6.0 * k || y < g.hit_y + 48.0 * k {
                continue;
            }
            let depth = (offset.max(0.0) / 1.6).clamp(0.0, 1.0);
            let size = (44.0 - 22.0 * depth) * k;
            // Completed words fade out as they slide above the current slot
            let row_alpha = if offset < 0.0 { (1.0 + offset).max(0.0) } else { 1.0 - 0.72 * depth };
            if row_alpha <= 0.01 {
                continue;
            }
            let word = &self.words[wi];
            let ws = self.word_starts[wi];
            let we = self.word_starts.get(wi + 1).copied().unwrap_or(self.notes.len());
            let letter_states = &self.notes[ws.min(we)..we];
            let gap = (6.0 - 2.5 * depth) * k;
            let total_w: f32 = word.chars().map(|c| msize(&c.to_string(), size).width + gap).sum();
            let mut x = g.left + g.width / 2.0 - total_w / 2.0 + ox;
            for (i, c) in word.chars().enumerate() {
                let up_next = next_letter == Some((wi, i));
                let mut color = match letter_states.get(i).map(|n| n.state) {
                    Some(NoteState::Hit(j)) => {
                        let mut c = j.color();
                        c.a = 0.9;
                        c
                    }
                    Some(NoteState::Missed) => wa(th().miss, 0.9),
                    _ if up_next => Color::new(1.0, 1.0, 1.0, 0.95),
                    _ => Color::new(1.0, 1.0, 1.0, 0.55),
                };
                color.a *= row_alpha;
                let s = c.to_string();
                dtext(&s, x, y, size, color);
                let w = msize(&s, size).width;
                if up_next {
                    // Soft accent underline marks where to re-anchor
                    let uy = y + 7.0 * k;
                    draw_line(x, uy, x + w, uy, 2.0 * k, wa(th().accent, 0.7 * row_alpha));
                }
                x += w + gap;
            }
        }

        // Side-gutter HUD: the score column lives in the left gutter, the
        // song column in the right — nothing overlays the highway. Text
        // shrinks to fit the gutter so long titles never spill onto it.
        let lcx = g.left / 2.0;
        let rcx = g.left + g.width + (screen_width() - g.left - g.width) / 2.0;
        let col_w = (g.left - 28.0 * k).max(60.0 * k);
        let sp_on = self.sp_active(now);
        // Gutter text goes gold with the rest of the star power dressing
        let gut = |a: f32| {
            if sp_on && sp_fx() {
                wa(th().accent, a)
            } else {
                Color::new(1.0, 1.0, 1.0, a)
            }
        };

        draw_fit("SCORE", lcx, 106.0 * k, 15.0 * k, col_w, gut(0.35));
        draw_fit(&format!("{}", self.score), lcx, 146.0 * k, 42.0 * k, col_w, gut(1.0));
        let mult_color = if sp_on { wa(th().accent, 1.0) } else { wa(th().accent, 0.9) };
        draw_fit(
            &format!("x{}", self.multiplier(now)),
            lcx,
            180.0 * k,
            24.0 * k,
            col_w,
            mult_color,
        );

        // Star power: the sparks carry the mood — the only steady marker
        // is the strike line turning gold, flickering nervously through
        // the last second and a half so the player feels the drain
        if sp_on {
            let remaining = (self.sp_until - now) as f32;
            let flicker = if remaining < 1.5 {
                let ph = ((now * 11.0).sin() * 0.5 + 0.5) as f32;
                (remaining / 1.5).clamp(0.0, 1.0) * (0.45 + 0.55 * ph) + 0.25
            } else {
                1.0
            };
            draw_strike_line(&g, ox, oy, 4.0 * k, wa(th().accent, 0.8 * flicker));
            // The highway edges catch the same gold, flickering in step
            if sp_fx() {
                let edge = wa(th().accent, 0.8 * flicker);
                for x in [g.left + ox, g.left + g.width + ox] {
                    draw_line(x, 0.0, x, h, 3.0 * k, edge);
                }
            }
        }
        // Ignition: one soft gold pulse over the highway, gone in half a
        // second
        if self.sp_flash > 0.0 {
            draw_rectangle(g.left + ox, 0.0, g.width, h, wa(th().accent, 0.08 * self.sp_flash));
        }
        if self.energy > 0.0 || sp_on {
            let bar_w = col_w.min(170.0 * k);
            let bx = lcx - bar_w / 2.0;
            let by = 208.0 * k;
            draw_rectangle(bx, by, bar_w, 8.0 * k, Color::new(1.0, 1.0, 1.0, 0.12));
            let fill = if sp_on {
                ((self.sp_until - now) / 16.0).clamp(0.0, 1.0) as f32
            } else {
                self.energy
            };
            let c = if sp_on || self.energy >= 0.5 {
                wa(th().accent, 0.95)
            } else {
                wa(th().accent, 0.45)
            };
            draw_rectangle(bx, by, bar_w * fill, 8.0 * k, c);
            if self.energy >= 0.5 && !sp_on {
                draw_fit(
                    "SPACE: star power",
                    lcx,
                    by + 26.0 * k,
                    16.0 * k,
                    col_w,
                    wa(th().accent, 0.8),
                );
            }
        }

        draw_fit(&self.title, rcx, 106.0 * k, 22.0 * k, col_w, gut(0.85));
        draw_fit(&self.diff_name, rcx, 130.0 * k, 16.0 * k, col_w, gut(0.45));
        let acc_text = format!("{:.1} %", self.accuracy());
        draw_fit(&acc_text, rcx, 174.0 * k, 30.0 * k, col_w, gut(0.85));

        // Song completion, down in the right gutter instead of across the top
        let resolved = self.notes.iter().filter(|n| n.state != NoteState::Pending).count();
        let frac = resolved as f32 / self.notes.len().max(1) as f32;
        let pw = col_w.min(170.0 * k);
        let py = 204.0 * k;
        draw_rectangle(rcx - pw / 2.0, py, pw, 4.0 * k, Color::new(1.0, 1.0, 1.0, 0.12));
        draw_rectangle(rcx - pw / 2.0, py, pw * frac, 4.0 * k, wa(th().secondary, 0.8));
        let pct = format!("{:.0}%", frac * 100.0);
        draw_fit(&pct, rcx, 228.0 * k, 15.0 * k, col_w, gut(0.4));

        // Combo
        if self.combo >= 4 {
            let text = format!("{}", self.combo);
            let size = (64.0 + (self.combo.min(60) as f32) * 0.4) * k;
            let dims = msize(&text, size);
            dtext(
                &text,
                g.left + g.width / 2.0 - dims.width / 2.0 + ox,
                g.hit_y - 130.0 * k + oy,
                size,
                Color::new(1.0, 1.0, 1.0, 0.16),
            );
        }

        // Count-in
        if now < self.first_note_time {
            let beats_left = (self.first_note_time - now) / self.spb;
            let text = if beats_left > 4.0 {
                "READY".to_string()
            } else {
                format!("{}", beats_left.ceil() as i64)
            };
            let size = 80.0 * k;
            let dims = msize(&text, size);
            dtext(
                &text,
                g.left + g.width / 2.0 - dims.width / 2.0,
                h * 0.4,
                size,
                Color::new(1.0, 1.0, 1.0, 0.5 + 0.5 * self.beat_flash),
            );
        }
    }
}

// ---------------------------------------------------------------- results

pub struct Results {
    pub song_ref: SongRef,
    pub title: String,
    pub diff_name: String,
    pub score: i64,
    pub max_combo: i64,
    pub perfect: u32,
    pub great: u32,
    pub good: u32,
    pub miss: u32,
    pub strays: u32,
    pub accuracy: f64,
    // Whether this run beat a stored personal best (a first clear doesn't
    // count), and the score it beat, for the results banner.
    pub new_best: bool,
    pub prev_best: Option<i64>,
}

impl Results {
    pub fn grade(&self) -> (&'static str, Color) {
        let total = (self.perfect + self.great + self.good + self.miss) as f64;
        let weighted = if total == 0.0 {
            0.0
        } else {
            (self.perfect as f64 * 100.0 + self.great as f64 * 80.0 + self.good as f64 * 50.0)
                / (total * 100.0)
                * 100.0
        };
        match weighted as i64 {
            93..=100 => ("S", wa(th().accent, 1.0)),
            85..=92 => ("A", Color::new(0.35, 0.9, 0.5, 1.0)),
            70..=84 => ("B", wa(th().secondary, 1.0)),
            50..=69 => ("C", Color::new(0.9, 0.6, 0.3, 1.0)),
            _ => ("D", Color::new(0.9, 0.35, 0.35, 1.0)),
        }
    }
}

#[cfg(test)]
mod phrase_tests {
    use super::*;

    /// Feed split_run a gap pattern and get back the phrase sizes it deals.
    fn split_gaps(gaps: &[f64]) -> Vec<usize> {
        let mut t = 0.0;
        let mut run = vec![(0.0, 0.0)];
        for &g in gaps {
            t += g;
            run.push((t, 0.0));
        }
        let mut out = Vec::new();
        split_run(run, &mut out);
        out.iter().map(|g| g.len()).collect()
    }

    #[test]
    fn even_streams_still_deal_eights() {
        // No internal structure: the classic 8-letter cut is unchanged
        assert_eq!(split_gaps(&[0.19; 68]), vec![8, 8, 8, 8, 8, 8, 8, 8, 5]);
        assert_eq!(split_gaps(&[0.19; 7]), vec![8]);
        assert_eq!(split_gaps(&[0.5; 11]), vec![8, 4]);
    }

    #[test]
    fn triplet_cells_deal_cell_multiples() {
        // Code Monkey hard: 3-note cells, seam gap twice the inner gap.
        // Words end at cell boundaries (6s), never straddling into the next
        let mut gaps = Vec::new();
        for _ in 0..7 {
            gaps.extend([0.19, 0.19, 0.38]);
        }
        gaps.extend([0.19, 0.19]); // final cell ends at the rest
        assert_eq!(split_gaps(&gaps), vec![6, 6, 6, 6]);
    }

    #[test]
    fn cuts_land_on_seams_not_mid_figure() {
        // A tight 4-note figure among slower notes: the cut may not split it
        let gaps = [0.56, 0.19, 0.56, 0.19, 0.56, 0.19, 0.19, 0.19, 0.38, 0.56, 0.19, 0.56, 0.19];
        let lens = split_gaps(&gaps);
        assert_eq!(lens.iter().sum::<usize>(), gaps.len() + 1);
        // Every word boundary sits on a seam (a gap > 0.28), never inside
        // the 0.19-gap figure at notes 5..=8
        let mut i = 0;
        for &l in &lens[..lens.len() - 1] {
            i += l;
            assert!(gaps[i - 1] > 0.28, "cut after note {} lands mid-figure", i - 1);
        }
    }

    #[test]
    fn degenerate_runs_fall_back_to_eights() {
        // One stray flam makes every other gap read as a seam: farthest
        // "seam" in range is just the full word, so eights survive
        let mut gaps = vec![0.05];
        gaps.extend([0.19; 20]);
        assert_eq!(split_gaps(&gaps), vec![8, 8, 6]);
    }

    /// The motivating chart: Code Monkey on hard is triplet cells. The dealt
    /// phrases must end on cell boundaries, not straddle them the way blind
    /// 8-cuts did (this loads the bundled .sng, skipped if absent).
    #[test]
    fn code_monkey_hard_words_ride_whole_triplets() {
        let path = std::path::Path::new("songs/Code Monkey.sng");
        if !path.exists() {
            return;
        }
        let source = crate::chart::SongSource::Sng(path.to_path_buf());
        let charts = crate::chart::load_song(&source).expect("bundled song should parse");
        let chart = crate::chart::pick_chart(charts, crate::chart::Instrument::Guitar)
            .expect("guitar chart");
        let times: Vec<(f64, f64)> = chart.diffs[2].iter().map(|n| (n.time, n.len)).collect();
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        for &(t, len) in &times {
            let new_run = match runs.last().and_then(|g| g.last()) {
                Some(&(prev, _)) => t - prev > 0.85,
                None => true,
            };
            if new_run {
                runs.push(Vec::new());
            }
            runs.last_mut().unwrap().push((t, len));
        }
        for run in runs.into_iter().filter(|r| r.len() > MAX_WORD) {
            let gaps: Vec<f64> = run.windows(2).map(|w| w[1].0 - w[0].0).collect();
            let mut out = Vec::new();
            split_run(run, &mut out);
            let mut i = 0;
            for g in &out[..out.len() - 1] {
                i += g.len();
                // Every cut lands on a wide gap, never inside a triplet cell
                assert!(
                    gaps[i - 1] > 0.28,
                    "cut after note {i} splits a cell (gap {:.2})",
                    gaps[i - 1]
                );
            }
        }
    }
}
