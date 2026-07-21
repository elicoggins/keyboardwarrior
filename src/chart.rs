// Clone Hero song loading: parses RB-style .mid and text .chart note charts
// plus song.ini metadata into a common SongChart. The 5-fret guitar and bass
// tracks are read as separate playable charts (the player picks which);
// what we take from a chart is the charter's *timing* — which beats carry
// notes — plus star power phrases and the tempo map.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;

use crate::sng::SngFile;

pub const DIFF_NAMES: [&str; 4] = ["EASY", "MEDIUM", "HARD", "EXPERT"];

#[derive(Clone, Copy)]
pub struct ChartNote {
    pub time: f64, // seconds from chart zero
    pub len: f64,  // sustain length in seconds (0 for a tap)
}

/// A playable 5-fret track. A song can chart more than one; the player picks
/// which to play, and it also decides which audio stem ducks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Instrument {
    Guitar,
    Bass,
}

impl Instrument {
    pub fn label(self) -> &'static str {
        match self {
            Instrument::Guitar => "GUITAR",
            Instrument::Bass => "BASS",
        }
    }
}

pub struct SongChart {
    pub title: String,
    pub artist: String,
    pub instrument: Instrument,
    pub diffs: [Vec<ChartNote>; 4], // easy, medium, hard, expert
    pub sp: [Vec<(f64, f64)>; 4],   // star power phrases (start, end) per difficulty
    pub beats: Vec<f64>,            // beat times for grid/pulse rendering
    pub end: f64,
}

/// A song must chart at least this many notes at a difficulty for that
/// difficulty to count as playable (skips near-empty stub tracks).
pub const MIN_NOTES: usize = 20;

/// When a difficulty is charted for more than one instrument, a track with
/// fewer than this fraction of the fullest track's notes is treated as the
/// sparse/incidental one and dropped, so the picker only offers genuinely
/// comparable charts.
const DOMINANCE_RATIO: f64 = 0.5;

/// What one instrument charts for a song: note count per difficulty (0 where
/// unavailable). Lives on SongEntry so the menu can decide, per difficulty,
/// whether to offer a chart picker without loading the whole song.
#[derive(Clone, Copy)]
pub struct ChartInfo {
    pub instrument: Instrument,
    pub counts: [usize; 4],
}

/// Which instruments to offer for a chosen difficulty. Returns every charted
/// instrument, minus any that are sparse next to the fullest (the dominance
/// rule) — so a single entry means "just play it", two means "let them pick".
pub fn charts_for_diff(charts: &[ChartInfo], diff: usize) -> Vec<Instrument> {
    let candidates: Vec<&ChartInfo> =
        charts.iter().filter(|c| c.counts[diff] >= MIN_NOTES).collect();
    let max = candidates.iter().map(|c| c.counts[diff]).max().unwrap_or(0);
    candidates
        .into_iter()
        .filter(|c| c.counts[diff] as f64 >= max as f64 * DOMINANCE_RATIO)
        .map(|c| c.instrument)
        .collect()
}

/// Where a song lives: an unpacked folder, or a .sng straight from
/// Chorus Encore. The browser demo has no filesystem, so there a song is a
/// .sng held in memory after being fetched over HTTP.
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq))]
// On wasm the filesystem variants are never built, but they stay compiled so
// the loading pipeline is one body of code across both targets.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum SongSource {
    Folder(PathBuf),
    Sng(PathBuf),
    #[cfg(target_arch = "wasm32")]
    Bytes(std::sync::Arc<Vec<u8>>),
}

/// On wasm the Bytes variant compares by identity — the demo song is fetched
/// once and shared, and a 13 MB content compare per cache check would be
/// wasteful.
#[cfg(target_arch = "wasm32")]
impl PartialEq for SongSource {
    fn eq(&self, other: &SongSource) -> bool {
        match (self, other) {
            (SongSource::Folder(a), SongSource::Folder(b)) => a == b,
            (SongSource::Sng(a), SongSource::Sng(b)) => a == b,
            (SongSource::Bytes(a), SongSource::Bytes(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

pub struct SongEntry {
    pub source: SongSource,
    pub title: String,
    pub artist: String,
    pub available: Vec<usize>, // difficulty indices playable by at least one chart
    pub charts: Vec<ChartInfo>, // per-instrument note counts, for the chart picker
    // A non-playable signpost row (the browser demo's "download to expand
    // library" entry): drawn dimmed, skipped by menu navigation.
    pub locked: bool,
    // A song that's on disk but failed to load (bad chart, no playable
    // difficulty). It still shows in the menu — dimmed, with this message —
    // so it can be selected and deleted; it just can't be played.
    pub error: Option<String>,
}

// ---------------------------------------------------------------- tempo map

struct TempoMap {
    // (tick, seconds at tick, seconds per tick after this point)
    points: Vec<(u64, f64, f64)>,
}

impl TempoMap {
    /// `tempos`: (tick, microseconds per quarter note); `tpq`: ticks/quarter.
    fn new(mut tempos: Vec<(u64, f64)>, tpq: f64) -> Self {
        tempos.sort_by_key(|t| t.0);
        if tempos.first().is_none_or(|t| t.0 > 0) {
            tempos.insert(0, (0, 500_000.0)); // 120 BPM default
        }
        let mut points = Vec::new();
        let mut sec = 0.0;
        let mut last_tick = 0u64;
        let mut spt = 500_000.0 / 1e6 / tpq;
        for (tick, us_per_qn) in tempos {
            sec += (tick - last_tick) as f64 * spt;
            last_tick = tick;
            spt = us_per_qn / 1e6 / tpq;
            points.push((tick, sec, spt));
        }
        TempoMap { points }
    }

    fn sec(&self, tick: u64) -> f64 {
        let i = self.points.partition_point(|p| p.0 <= tick).max(1) - 1;
        let (t0, s0, spt) = self.points[i];
        s0 + (tick.saturating_sub(t0)) as f64 * spt
    }
}

fn beats_until(map: &TempoMap, tpq: f64, max_tick: u64) -> Vec<f64> {
    let mut beats = Vec::new();
    let mut tick = 0u64;
    while tick <= max_tick {
        beats.push(map.sec(tick));
        tick += tpq as u64;
    }
    beats
}

// ---------------------------------------------------------------- .mid

pub fn parse_mid(bytes: &[u8], delay: f64) -> Result<Vec<SongChart>, String> {
    let smf = midly::Smf::parse(bytes).map_err(|e| format!("bad MIDI: {e}"))?;
    let tpq = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int() as f64,
        _ => return Err("SMPTE-timed MIDI is not supported".into()),
    };

    // Tempo events can appear in any track (normally the first)
    let mut tempos = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0u64;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            if let midly::TrackEventKind::Meta(midly::MetaMessage::Tempo(us)) = ev.kind {
                tempos.push((tick, us.as_int() as f64));
            }
        }
    }
    let map = TempoMap::new(tempos, tpq);

    // Collect the playable 5-fret tracks (guitar and bass). A song can chart a
    // track more than once; per instrument we keep the one that carries the
    // song: earliest entry wins (a track that sits out the intro riff loses to
    // one playing from the top), ties go to density.
    struct TrackData {
        instrument: Instrument,
        notes: Vec<(u64, u8, u64)>, // (tick, midi key, length in ticks)
        sp: Vec<(u64, u64)>,
        first: u64,
    }
    let mut candidates: Vec<TrackData> = Vec::new();
    for track in &smf.tracks {
        let mut name = String::new();
        let mut tick = 0u64;
        let mut notes: Vec<(u64, u8, u64)> = Vec::new();
        // Open note-ons per key, so note-offs give sustain lengths
        let mut open = [usize::MAX; 128];
        let mut sp_start: Option<u64> = None;
        let mut local_sp = Vec::new();
        for ev in track {
            tick += ev.delta.as_int() as u64;
            match ev.kind {
                midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) => {
                    name = String::from_utf8_lossy(n).to_string();
                }
                midly::TrackEventKind::Midi { message, .. } => match message {
                    midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                        let k = key.as_int();
                        if k == 116 {
                            sp_start = Some(tick);
                        } else {
                            open[k as usize] = notes.len();
                            notes.push((tick, k, 0));
                        }
                    }
                    midly::MidiMessage::NoteOn { key, .. }
                    | midly::MidiMessage::NoteOff { key, .. } => {
                        let k = key.as_int() as usize;
                        if k == 116 {
                            if let Some(s) = sp_start.take() {
                                local_sp.push((s, tick));
                            }
                        } else if open[k] != usize::MAX {
                            notes[open[k]].2 = tick.saturating_sub(notes[open[k]].0);
                            open[k] = usize::MAX;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let instrument = match name.trim() {
            "PART GUITAR" => Instrument::Guitar,
            "PART BASS" => Instrument::Bass,
            _ => continue,
        };
        if !notes.is_empty() {
            let first = notes.iter().map(|n| n.0).min().unwrap_or(u64::MAX);
            candidates.push(TrackData { instrument, notes, sp: local_sp, first });
        }
    }

    // Turn one instrument's best track into a SongChart, or None if it charts
    // no notes in any difficulty.
    let build = |instrument: Instrument| -> Option<SongChart> {
        let best = candidates
            .iter()
            .filter(|c| c.instrument == instrument)
            .min_by(|a, b| a.first.cmp(&b.first).then(b.notes.len().cmp(&a.notes.len())))?;

        let mut diffs: [Vec<ChartNote>; 4] = Default::default();
        let mut max_tick = 0u64;
        // Difficulty note ranges: Easy 60–64, Medium 72–76, Hard 84–88, Expert 96–100
        for &(t, k, lt) in &best.notes {
            let d = match k {
                60..=64 => 0,
                72..=76 => 1,
                84..=88 => 2,
                96..=100 => 3,
                _ => continue,
            };
            max_tick = max_tick.max(t);
            let time = map.sec(t) + delay;
            let len = (map.sec(t + lt) - map.sec(t)).max(0.0);
            // Collapse chords: one gem per tick, keeping the longest sustain
            if diffs[d].last().is_none_or(|n: &ChartNote| n.time < time - 1e-9) {
                diffs[d].push(ChartNote { time, len });
            } else if let Some(last) = diffs[d].last_mut() {
                last.len = last.len.max(len);
            }
        }
        if diffs.iter().all(|d| d.is_empty()) {
            return None;
        }

        let sp_secs: Vec<(f64, f64)> =
            best.sp.iter().map(|&(a, b)| (map.sec(a) + delay, map.sec(b) + delay)).collect();
        let sp = [sp_secs.clone(), sp_secs.clone(), sp_secs.clone(), sp_secs];

        let beats = beats_until(&map, tpq, max_tick + (tpq as u64) * 8);
        let end = diffs.iter().flat_map(|d| d.iter()).map(|n| n.time + n.len).fold(0.0, f64::max);
        Some(SongChart {
            title: String::new(),
            artist: String::new(),
            instrument,
            diffs,
            sp,
            beats,
            end,
        })
    };

    let charts: Vec<SongChart> =
        [Instrument::Guitar, Instrument::Bass].into_iter().filter_map(build).collect();
    if charts.is_empty() {
        return Err("no PART GUITAR or PART BASS track with notes".into());
    }
    Ok(charts)
}

// ---------------------------------------------------------------- .chart

pub fn parse_chart(text: &str, delay: f64) -> Result<Vec<SongChart>, String> {
    // Split into [Section] { ... } blocks
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            if let Some(s) = current.take() {
                sections.push(s);
            }
            current = Some((l[1..l.len() - 1].to_string(), Vec::new()));
        } else if l != "{" && l != "}" && !l.is_empty() {
            if let Some((_, lines)) = current.as_mut() {
                lines.push(l);
            }
        }
    }
    if let Some(s) = current.take() {
        sections.push(s);
    }
    let get = |name: &str| sections.iter().find(|(n, _)| n == name).map(|(_, l)| l);

    let mut resolution = 192.0;
    let mut title = String::new();
    let mut artist = String::new();
    if let Some(song) = get("Song") {
        for l in song {
            let Some((k, v)) = l.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            match k {
                "Resolution" => resolution = v.parse().unwrap_or(192.0),
                "Name" => title = v.to_string(),
                "Artist" => artist = v.to_string(),
                _ => {}
            }
        }
    }

    let mut tempos = Vec::new();
    if let Some(sync) = get("SyncTrack") {
        for l in sync {
            let Some((tick, rest)) = l.split_once('=') else { continue };
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == "B" {
                if let (Ok(t), Ok(bpm1000)) = (tick.trim().parse::<u64>(), parts[1].parse::<f64>())
                {
                    tempos.push((t, 60e6 / (bpm1000 / 1000.0)));
                }
            }
        }
    }
    let map = TempoMap::new(tempos, resolution);

    let mut diffs: [Vec<ChartNote>; 4] = Default::default();
    let mut sp: [Vec<(f64, f64)>; 4] = Default::default();
    let mut max_tick = 0u64;
    for (d, prefix) in ["Easy", "Medium", "Hard", "Expert"].iter().enumerate() {
        let Some(lines) = get(&format!("{}Single", prefix)) else { continue };
        let mut last_tick = u64::MAX;
        for l in lines {
            let Some((tick, rest)) = l.split_once('=') else { continue };
            let Ok(t) = tick.trim().parse::<u64>() else { continue };
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "N" {
                let fret: u8 = parts[1].parse().unwrap_or(99);
                // 0–4 are frets, 7 is open; 5/6 are HOPO/tap modifiers
                if fret <= 4 || fret == 7 {
                    let lt: u64 = parts[2].parse().unwrap_or(0);
                    let len = (map.sec(t + lt) - map.sec(t)).max(0.0);
                    if t != last_tick {
                        diffs[d].push(ChartNote { time: map.sec(t) + delay, len });
                        last_tick = t;
                        max_tick = max_tick.max(t);
                    } else if let Some(last) = diffs[d].last_mut() {
                        // Chords collapse to one gem; keep the longest sustain
                        last.len = last.len.max(len);
                    }
                }
            } else if parts.len() >= 3 && parts[0] == "S" && parts[1] == "2" {
                if let Ok(len) = parts[2].parse::<u64>() {
                    sp[d].push((map.sec(t) + delay, map.sec(t + len) + delay));
                }
            }
        }
    }
    if diffs.iter().all(|d| d.is_empty()) {
        return Err("no [..Single] guitar sections with notes".into());
    }

    let beats = beats_until(&map, resolution, max_tick + resolution as u64 * 8);
    let end = diffs.iter().flat_map(|d| d.iter()).map(|n| n.time + n.len).fold(0.0, f64::max);
    // The .chart "Single" track is lead guitar by definition — this format
    // carries no bass chart the game reads, so there's only ever one option.
    Ok(vec![SongChart { title, artist, instrument: Instrument::Guitar, diffs, sp, beats, end }])
}

// ---------------------------------------------------------------- loading

/// Metadata from ini-style key/value pairs: (title, artist, delay seconds).
fn meta_from_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> (String, String, f64) {
    let mut title = String::new();
    let mut artist = String::new();
    let mut delay = 0.0;
    for (k, v) in pairs {
        match k.trim().to_lowercase().as_str() {
            "name" => title = v.trim().to_string(),
            "artist" => artist = v.trim().to_string(),
            "delay" => delay = v.trim().parse::<f64>().unwrap_or(0.0) / 1000.0,
            _ => {}
        }
    }
    (title, artist, delay)
}

/// Metadata, notes.mid bytes, and notes.chart text out of an opened .sng.
type SngNotes = (String, String, f64, Option<Vec<u8>>, Option<String>);

fn sng_notes(sng: &SngFile) -> Result<SngNotes, String> {
    let (t, a, d) = meta_from_pairs(sng.metadata.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let mid = if sng.has("notes.mid") { Some(sng.read("notes.mid")?) } else { None };
    let chart_text = if mid.is_none() && sng.has("notes.chart") {
        Some(String::from_utf8_lossy(&sng.read("notes.chart")?).to_string())
    } else {
        None
    };
    Ok((t, a, d, mid, chart_text))
}

/// Every playable chart (one per instrument) for a song, sharing its metadata.
pub fn load_song(source: &SongSource) -> Result<Vec<SongChart>, String> {
    let (title, artist, delay, mid, chart_text) = match source {
        SongSource::Folder(dir) => {
            let ini = std::fs::read_to_string(dir.join("song.ini")).unwrap_or_default();
            let (t, a, d) = meta_from_pairs(ini.lines().filter_map(|l| l.split_once('=')));
            let mid = std::fs::read(dir.join("notes.mid")).ok();
            let chart_text = if mid.is_some() {
                None
            } else {
                std::fs::read_to_string(dir.join("notes.chart")).ok()
            };
            (t, a, d, mid, chart_text)
        }
        #[cfg(not(target_arch = "wasm32"))]
        SongSource::Sng(path) => sng_notes(&SngFile::open(path)?)?,
        #[cfg(target_arch = "wasm32")]
        SongSource::Sng(_) => return Err("filesystem songs aren't available in the browser".into()),
        #[cfg(target_arch = "wasm32")]
        SongSource::Bytes(bytes) => sng_notes(&SngFile::from_bytes(bytes.clone())?)?,
    };

    let mut charts = if let Some(bytes) = mid {
        parse_mid(&bytes, delay)?
    } else if let Some(text) = chart_text {
        parse_chart(&text, delay)?
    } else {
        return Err("no notes.mid or notes.chart".into());
    };
    for chart in &mut charts {
        if !title.is_empty() {
            chart.title = title.clone();
        }
        if !artist.is_empty() {
            chart.artist = artist.clone();
        }
    }
    Ok(charts)
}

/// The chart matching `instrument`, or the first available as a fallback (a
/// requested instrument can go missing if the library changed under us).
pub fn pick_chart(charts: Vec<SongChart>, instrument: Instrument) -> Option<SongChart> {
    let mut charts = charts;
    let idx = charts.iter().position(|c| c.instrument == instrument).unwrap_or(0);
    if charts.is_empty() {
        None
    } else {
        Some(charts.swap_remove(idx))
    }
}

/// Note counts per instrument per difficulty for a set of charts — the summary
/// SongEntry stores so the menu can drive the chart picker.
pub fn chart_infos(charts: &[SongChart]) -> Vec<ChartInfo> {
    charts
        .iter()
        .map(|c| ChartInfo {
            instrument: c.instrument,
            counts: std::array::from_fn(|d| c.diffs[d].len()),
        })
        .collect()
}

/// Difficulty indices playable by at least one of the given charts.
pub fn available_diffs(charts: &[SongChart]) -> Vec<usize> {
    (0..4).filter(|&d| charts.iter().any(|c| c.diffs[d].len() >= MIN_NOTES)).collect()
}

const AUDIO_EXTS: [&str; 4] = ["opus", "ogg", "mp3", "wav"];

fn is_stem_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    let Some((base, ext)) = lower.rsplit_once('.') else { return false };
    AUDIO_EXTS.contains(&ext) && !matches!(base, "crowd" | "preview") && !base.starts_with("crowd")
}

/// All audio stems for a song as (filename, bytes).
pub fn stem_files(source: &SongSource) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    match source {
        SongSource::Folder(dir) => {
            let read = std::fs::read_dir(dir).map_err(|e| format!("cannot read folder: {e}"))?;
            for e in read.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if is_stem_name(&name) {
                    let bytes = std::fs::read(e.path()).map_err(|e| format!("{name}: {e}"))?;
                    out.push((name, bytes));
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        SongSource::Sng(path) => sng_stems(&SngFile::open(path)?, &mut out)?,
        #[cfg(target_arch = "wasm32")]
        SongSource::Sng(_) => return Err("filesystem songs aren't available in the browser".into()),
        #[cfg(target_arch = "wasm32")]
        SongSource::Bytes(bytes) => sng_stems(&SngFile::from_bytes(bytes.clone())?, &mut out)?,
    }
    Ok(out)
}

fn sng_stems(sng: &SngFile, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let names: Vec<String> = sng.file_names().cloned().collect();
    for name in names {
        if is_stem_name(&name) {
            let bytes = sng.read(&name)?;
            out.push((name, bytes));
        }
    }
    Ok(())
}

/// Stem base names that carry the charted instrument, in preference order.
pub fn lead_stem_names(instrument: Instrument) -> &'static [&'static str] {
    match instrument {
        Instrument::Guitar => &["guitar"],
        // RB-era rips often store bass audio under "rhythm"
        Instrument::Bass => &["bass", "rhythm"],
    }
}

/// Scan songs/ for songs: unpacked folders and raw .sng files. Every
/// song-shaped thing becomes an entry — one that failed to load carries an
/// `error` and no difficulties, so failures aren't silent and the player can
/// still see and delete the offending file from the menu.
/// (Native only — the browser demo's fixed library is built in web.rs.)
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_songs(root: &Path) -> Vec<SongEntry> {
    let mut entries = Vec::new();
    let Ok(read) = std::fs::read_dir(root) else { return entries };
    for e in read.flatten() {
        let path = e.path();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let source = if path.is_dir() {
            SongSource::Folder(path)
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("sng")) {
            SongSource::Sng(path)
        } else {
            continue;
        };
        // A song that fails to load still becomes a (dimmed, unplayable) entry
        // so the player can see and delete it from the menu.
        let broken = |source, msg: String| SongEntry {
            charts: Vec::new(),
            title: name.clone(),
            artist: String::new(),
            available: Vec::new(),
            source,
            locked: false,
            error: Some(msg),
        };
        match load_song(&source) {
            Ok(charts) => {
                let available = available_diffs(&charts);
                if available.is_empty() {
                    entries.push(broken(source, "no difficulty with enough notes".into()));
                    continue;
                }
                let (title, artist) =
                    charts.first().map(|c| (c.title.clone(), c.artist.clone())).unwrap_or_default();
                entries.push(SongEntry {
                    title: if title.is_empty() { "Unknown".into() } else { title },
                    artist,
                    available,
                    charts: chart_infos(&charts),
                    source,
                    locked: false,
                    error: None,
                });
            }
            Err(err) => entries.push(broken(source, err)),
        }
    }
    // The user's own songs first (alphabetical), bundled songs at the bottom
    let bundled = |e: &SongEntry| {
        let (SongSource::Folder(p) | SongSource::Sng(p)) = &e.source;
        p.file_name().is_some_and(|n| BUNDLED.contains(&n.to_string_lossy().as_ref()))
    };
    entries.sort_by(|a, b| bundled(a).cmp(&bundled(b)).then_with(|| a.title.cmp(&b.title)));
    entries
}

/// Scan several song roots at once and merge the results. The bundled songs/
/// dir is expected first in `roots`; user-added folders follow. A song reachable
/// from more than one root (identical source path) is kept only once, and the
/// bundled-sinks-to-bottom sort still holds across the merged list so the
/// defaults stay below the player's own songs.
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_all(roots: &[PathBuf]) -> Vec<SongEntry> {
    let mut entries: Vec<SongEntry> = Vec::new();
    for root in roots {
        for e in scan_songs(root) {
            if !entries.iter().any(|x| x.source == e.source) {
                entries.push(e);
            }
        }
    }
    // Re-apply the bundled-to-bottom ordering over the combined list (scan_songs
    // already sorted each root, but the merge interleaves roots).
    let bundled = |e: &SongEntry| {
        let (SongSource::Folder(p) | SongSource::Sng(p)) = &e.source;
        p.file_name().is_some_and(|n| BUNDLED.contains(&n.to_string_lossy().as_ref()))
    };
    entries.sort_by(|a, b| bundled(a).cmp(&bundled(b)).then_with(|| a.title.cmp(&b.title)));
    entries
}

/// The freely-licensed songs committed to the repo (see songs/README.md).
/// They sort below user-added songs, so a growing library stays on top.
#[cfg(not(target_arch = "wasm32"))]
const BUNDLED: [&str; 3] =
    ["Code Monkey.sng", "Discipline.sng", "This Week I've Been Mostly Playing Guitar.sng"];

/// Whether a source is one of the repo's committed default songs — those are
/// protected from in-game deletion so the checkout's defaults can't be removed.
#[cfg(not(target_arch = "wasm32"))]
pub fn is_bundled(source: &SongSource) -> bool {
    let (SongSource::Folder(p) | SongSource::Sng(p)) = source;
    p.file_name().is_some_and(|n| BUNDLED.contains(&n.to_string_lossy().as_ref()))
}

/// Delete a song from disk: removes the `.sng` file or unpacks the song folder.
/// Refuses to touch a bundled default. Used by the menu's delete action.
#[cfg(not(target_arch = "wasm32"))]
pub fn delete_song(source: &SongSource) -> Result<(), String> {
    if is_bundled(source) {
        return Err("that's a bundled song and can't be deleted".into());
    }
    match source {
        SongSource::Sng(p) => std::fs::remove_file(p).map_err(|e| e.to_string()),
        SongSource::Folder(p) => std::fs::remove_dir_all(p).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod sng_tests {
    use super::*;

    /// The browser demo's repacked song (scripts/pack_demo_song.py) must
    /// stay loadable by the exact pipeline the demo uses: SNGPKG container,
    /// notes.mid chart, and two vorbis stems with guitar as the lead.
    #[test]
    fn demo_pack_loads() {
        let path = Path::new("web/songs/Code Monkey.sng");
        if !path.exists() {
            return;
        }
        let source = SongSource::Sng(path.to_path_buf());
        let charts = load_song(&source).expect("demo .sng should parse");
        let chart = pick_chart(charts, Instrument::Guitar).expect("guitar chart");
        assert_eq!(chart.title, "Code Monkey");
        assert_eq!(chart.instrument, Instrument::Guitar);
        assert!(chart.diffs.iter().all(|d| d.len() >= 20));
        let stems = stem_files(&source).expect("stems should read");
        assert_eq!(stems.len(), 2, "premixed backing + guitar lead");
        for (name, bytes) in &stems {
            let buf = crate::decode::decode(bytes, name, 48000).expect("vorbis should decode");
            let secs = buf.len() as f64 / 48000.0;
            println!("{name}: {secs:.1}s");
            assert!(secs > 180.0, "{name} should be a full-length stem");
        }
    }

    #[test]
    fn loads_sng_directly() {
        let path = Path::new("songs/Seven Nation Army.sng");
        if !path.exists() {
            return;
        }
        let source = SongSource::Sng(path.to_path_buf());
        let charts = load_song(&source).expect(".sng should parse");
        // This rip charts a bass line; keep the bass-stem test meaningful.
        let chart = pick_chart(charts, Instrument::Bass).expect("bass chart");
        assert_eq!(chart.title, "Seven Nation Army");
        assert_eq!(chart.instrument, Instrument::Bass);
        assert!(chart.diffs.iter().any(|d| d.len() > 100));
        let stems = stem_files(&source).expect("stems should read");
        println!(
            "sng stems: {:?}",
            stems.iter().map(|(n, b)| (n.clone(), b.len())).collect::<Vec<_>>()
        );
        assert!(stems.iter().any(|(n, _)| n.starts_with("rhythm")));
        // Decode one opus stem end-to-end
        let (name, bytes) = stems.iter().find(|(n, _)| n.starts_with("song")).unwrap();
        let buf = crate::decode::decode(bytes, name, 48000).expect("opus should decode");
        println!(
            "decoded {} -> {} frames ({:.1}s at 48k)",
            name,
            buf.len(),
            buf.len() as f64 / 48000.0
        );
        assert!(buf.len() as f64 / 48000.0 > 200.0, "should be a full-length song");
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn scans_all_songs() {
        let entries = scan_songs(Path::new("songs"));
        for e in &entries {
            match &e.error {
                Some(err) => println!("error: {} — {err}", e.title),
                None => println!("{} — {} (diffs {:?})", e.title, e.artist, e.available),
            }
        }
        // A loadable song always exposes at least one playable difficulty;
        // only broken entries (which carry an error) may have none.
        assert!(entries.iter().all(|e| e.error.is_some() || !e.available.is_empty()));
    }

    /// delete_song refuses bundled defaults (the guardrail) and removes a
    /// user-added .sng. Uses a temp copy so the real library is untouched.
    #[test]
    fn delete_protects_bundled_removes_user() {
        let sample = Path::new("songs/Code Monkey.sng");
        if !sample.exists() {
            return;
        }
        // Bundled default is protected regardless of where it sits.
        let bundled = SongSource::Sng(sample.to_path_buf());
        assert!(is_bundled(&bundled));
        assert!(delete_song(&bundled).is_err(), "bundled songs must not delete");

        // A user song (any non-bundled name) actually deletes.
        let tmp = std::env::temp_dir().join(format!("kw_del_{}.sng", std::process::id()));
        std::fs::copy(sample, &tmp).unwrap();
        let user = SongSource::Sng(tmp.clone());
        assert!(!is_bundled(&user));
        assert!(delete_song(&user).is_ok());
        assert!(!tmp.exists(), "file should be gone after delete");
    }

    /// scan_all merges the bundled dir with an extra folder that itself points
    /// back at songs/: every song appears once (dedup by source path) and the
    /// bundled defaults still sort to the bottom.
    #[test]
    fn scan_all_merges_and_dedupes() {
        let songs = Path::new("songs");
        if !songs.exists() {
            return;
        }
        let single = scan_songs(songs);
        // The same root twice must not double the library.
        let merged = scan_all(&[songs.to_path_buf(), songs.to_path_buf()]);
        assert_eq!(merged.len(), single.len(), "duplicate roots dedupe to one library");

        if !merged.is_empty() {
            // Once the bundled block starts, it never yields back to a
            // non-bundled song — defaults are pinned to the bottom.
            let is_bundled = |e: &SongEntry| {
                let (SongSource::Folder(p) | SongSource::Sng(p)) = &e.source;
                p.file_name().is_some_and(|n| BUNDLED.contains(&n.to_string_lossy().as_ref()))
            };
            let mut seen_bundled = false;
            for e in &merged {
                if is_bundled(e) {
                    seen_bundled = true;
                } else {
                    assert!(!seen_bundled, "a user song sorted below a bundled one");
                }
            }
        }
    }
}
