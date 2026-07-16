// Clone Hero song loading: parses RB-style .mid and text .chart note charts
// plus song.ini metadata into a common SongChart. Only 5-fret guitar tracks
// are read; what we take from a chart is the charter's *timing* — which beats
// carry notes — plus star power phrases and the tempo map.

use std::path::{Path, PathBuf};

use crate::sng::SngFile;

pub const DIFF_NAMES: [&str; 4] = ["EASY", "MEDIUM", "HARD", "EXPERT"];

#[derive(Clone, Copy)]
pub struct ChartNote {
    pub time: f64, // seconds from chart zero
    pub len: f64,  // sustain length in seconds (0 for a tap)
}

/// Which charted instrument the gems follow — decides which stem ducks.
#[derive(Clone, Copy, PartialEq)]
pub enum Instrument {
    Guitar,
    Bass,
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

/// Where a song lives: an unpacked folder, or a .sng straight from
/// Chorus Encore.
#[derive(Clone, PartialEq)]
pub enum SongSource {
    Folder(PathBuf),
    Sng(PathBuf),
}

pub struct SongEntry {
    pub source: SongSource,
    pub title: String,
    pub artist: String,
    pub available: Vec<usize>, // difficulty indices with notes
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

pub fn parse_mid(bytes: &[u8], delay: f64) -> Result<SongChart, String> {
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

    // Collect the 5-fret tracks we can play (guitar and bass), then keep the
    // one that carries the song: earliest entry wins (a track that sits out
    // the intro riff loses to one playing from the top), ties go to density.
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
    let best = candidates
        .into_iter()
        .min_by(|a, b| a.first.cmp(&b.first).then(b.notes.len().cmp(&a.notes.len())))
        .ok_or("no PART GUITAR or PART BASS track with notes")?;

    let mut diffs: [Vec<ChartNote>; 4] = Default::default();
    let mut max_tick = 0u64;
    // Difficulty note ranges: Easy 60–64, Medium 72–76, Hard 84–88, Expert 96–100
    for (t, k, lt) in best.notes {
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
    let sp_spans = best.sp;
    if diffs.iter().all(|d| d.is_empty()) {
        return Err("no notes in any difficulty".into());
    }

    let sp_secs: Vec<(f64, f64)> =
        sp_spans.iter().map(|&(a, b)| (map.sec(a) + delay, map.sec(b) + delay)).collect();
    let sp = [sp_secs.clone(), sp_secs.clone(), sp_secs.clone(), sp_secs];

    let beats = beats_until(&map, tpq, max_tick + (tpq as u64) * 8);
    let end = diffs.iter().flat_map(|d| d.iter()).map(|n| n.time + n.len).fold(0.0, f64::max);
    Ok(SongChart {
        title: String::new(),
        artist: String::new(),
        instrument: best.instrument,
        diffs,
        sp,
        beats,
        end,
    })
}

// ---------------------------------------------------------------- .chart

pub fn parse_chart(text: &str, delay: f64) -> Result<SongChart, String> {
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
    // The .chart "Single" track is lead guitar by definition
    Ok(SongChart { title, artist, instrument: Instrument::Guitar, diffs, sp, beats, end })
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

pub fn load_song(source: &SongSource) -> Result<SongChart, String> {
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
        SongSource::Sng(path) => {
            let sng = SngFile::open(path)?;
            let (t, a, d) =
                meta_from_pairs(sng.metadata.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let mid = if sng.has("notes.mid") { Some(sng.read("notes.mid")?) } else { None };
            let chart_text = if mid.is_none() && sng.has("notes.chart") {
                Some(String::from_utf8_lossy(&sng.read("notes.chart")?).to_string())
            } else {
                None
            };
            (t, a, d, mid, chart_text)
        }
    };

    let mut chart = if let Some(bytes) = mid {
        parse_mid(&bytes, delay)?
    } else if let Some(text) = chart_text {
        parse_chart(&text, delay)?
    } else {
        return Err("no notes.mid or notes.chart".into());
    };
    if !title.is_empty() {
        chart.title = title;
    }
    if !artist.is_empty() {
        chart.artist = artist;
    }
    Ok(chart)
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
        SongSource::Sng(path) => {
            let sng = SngFile::open(path)?;
            let names: Vec<String> = sng.file_names().cloned().collect();
            for name in names {
                if is_stem_name(&name) {
                    let bytes = sng.read(&name)?;
                    out.push((name, bytes));
                }
            }
        }
    }
    Ok(out)
}

/// Stem base names that carry the charted instrument, in preference order.
pub fn lead_stem_names(instrument: Instrument) -> &'static [&'static str] {
    match instrument {
        Instrument::Guitar => &["guitar"],
        // RB-era rips often store bass audio under "rhythm"
        Instrument::Bass => &["bass", "rhythm"],
    }
}

/// Scan songs/ for playable songs: unpacked folders and raw .sng files.
/// Returns the playable entries plus a human-readable reason for every
/// song-shaped thing that couldn't be loaded, so failures aren't silent.
pub fn scan_songs(root: &Path) -> (Vec<SongEntry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let Ok(read) = std::fs::read_dir(root) else { return (entries, errors) };
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
        match load_song(&source) {
            Ok(chart) => {
                let available: Vec<usize> =
                    (0..4).filter(|&d| chart.diffs[d].len() >= 20).collect();
                if available.is_empty() {
                    errors.push(format!("{name}: no difficulty with enough notes"));
                    continue;
                }
                entries.push(SongEntry {
                    title: if chart.title.is_empty() {
                        "Unknown".into()
                    } else {
                        chart.title.clone()
                    },
                    artist: chart.artist.clone(),
                    available,
                    source,
                });
            }
            Err(err) => errors.push(format!("{name}: {err}")),
        }
    }
    entries.sort_by(|a, b| a.title.cmp(&b.title));
    (entries, errors)
}

#[cfg(test)]
mod sng_tests {
    use super::*;

    #[test]
    fn loads_sng_directly() {
        let path = Path::new("songs/Seven Nation Army.sng");
        if !path.exists() {
            return;
        }
        let source = SongSource::Sng(path.to_path_buf());
        let chart = load_song(&source).expect(".sng should parse");
        assert_eq!(chart.title, "Seven Nation Army");
        assert!(chart.instrument == Instrument::Bass);
        assert!(chart.diffs[0].len() > 100);
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
        let (entries, errors) = scan_songs(Path::new("songs"));
        for e in &entries {
            println!("{} — {} (diffs {:?})", e.title, e.artist, e.available);
        }
        for e in &errors {
            println!("error: {e}");
        }
        if !entries.is_empty() {
            assert!(entries.iter().all(|e| !e.available.is_empty()));
        }
    }
}
