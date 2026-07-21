// On-disk player config — the app's first persisted state. Right now it holds
// just the list of *extra* song folders the player has pointed the game at
// (e.g. an existing Clone Hero library elsewhere on disk). The bundled songs/
// dir is always scanned on top of these; extra folders are additive, never a
// replacement.
//
// Native only: the browser demo has no filesystem and ships a fixed library.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;

use serde::{Deserialize, Serialize};

use crate::audio::AudioEngine;
use crate::settings::{CALIB_MS, SPEEDS, SPEED_IDX};
use crate::theme::{THEMES, THEME_IDX};
use crate::words::{
    PRAC_BOTTOM, PRAC_HOME, PRAC_LEFT, PRAC_PUNCT, PRAC_RIGHT, PRAC_TOP, TEXT_MODES, TEXT_MODE_IDX,
};

/// A file in the app's state directory (alongside config.toml). None when no
/// config directory can be resolved — persistence is simply skipped then.
fn state_file(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("keyboardwarrior").join(name))
}

/// Extra song roots plus the resolved config-file path they persist to.
pub struct Config {
    /// Absolute paths of user-added song folders, in the order added.
    pub song_dirs: Vec<PathBuf>,
    /// Where song_dirs is written back to; None if no writable location exists.
    path: Option<PathBuf>,
}

/// `dirs::config_dir()/keyboardwarrior/config.toml`, or a dotfile next to the
/// binary as a last resort. None only if neither location can be determined.
fn config_path() -> Option<PathBuf> {
    if let Some(dir) = dirs::config_dir() {
        return Some(dir.join("keyboardwarrior").join("config.toml"));
    }
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(".keyboardwarrior")))
}

impl Config {
    /// Load the config file (missing file = empty config) and merge in any
    /// paths from the KW_SONG_DIRS env var, so the folders are scriptable too.
    pub fn load() -> Self {
        let path = config_path();
        let mut song_dirs = match &path {
            Some(p) => std::fs::read_to_string(p).map(|s| parse_song_dirs(&s)).unwrap_or_default(),
            None => Vec::new(),
        };
        if let Some(env) = std::env::var_os("KW_SONG_DIRS") {
            for p in std::env::split_paths(&env) {
                push_unique(&mut song_dirs, p);
            }
        }
        Config { song_dirs, path }
    }

    /// Add a folder and persist. Returns false if it was already present (so
    /// the caller can report "already added" rather than a spurious success).
    pub fn add_song_dir(&mut self, dir: PathBuf) -> bool {
        let before = self.song_dirs.len();
        push_unique(&mut self.song_dirs, dir);
        if self.song_dirs.len() == before {
            return false;
        }
        self.save();
        true
    }

    /// Write song_dirs back to the config file, creating parent dirs as needed.
    /// Best-effort: a write failure is swallowed (the in-memory list still
    /// works for this session).
    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut out = String::from(
            "# Keyboard Warrior config. song_dirs are extra folders scanned for\n\
             # songs, on top of the bundled songs/ dir. One quoted path per line.\n\
             song_dirs = [\n",
        );
        for d in &self.song_dirs {
            // Paths are shown as-is; a double-quote in a path (rare) is escaped
            // so the file stays parseable.
            out.push_str(&format!("  \"{}\",\n", d.to_string_lossy().replace('"', "\\\"")));
        }
        out.push_str("]\n");
        let _ = std::fs::write(path, out);
    }
}

/// Pull quoted paths out of a `song_dirs = [ "..", ".." ]` block. Deliberately
/// tiny — the file is written by us and only ever holds this one flat array, so
/// a full TOML parser (and dependency) isn't warranted. Anything outside the
/// array, comments, and blank entries are ignored.
fn parse_song_dirs(s: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut in_array = false;
    for line in s.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if !in_array {
            // Enter the array when we see the key; a single-line
            // `song_dirs = ["a", "b"]` is handled by falling through.
            if let Some(rest) = line.strip_prefix("song_dirs") {
                in_array = true;
                collect_quoted(rest, &mut dirs);
                if rest.contains(']') {
                    break;
                }
            }
            continue;
        }
        collect_quoted(line, &mut dirs);
        if line.contains(']') {
            break;
        }
    }
    dirs
}

/// Extract every "double-quoted" segment on a line as a path.
fn collect_quoted(line: &str, out: &mut Vec<PathBuf>) {
    let mut rest = line;
    while let Some(open) = rest.find('"') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else { break };
        let raw = rest[..close].replace("\\\"", "\"");
        push_unique(out, PathBuf::from(raw));
        rest = &rest[close + 1..];
    }
}

fn push_unique(dirs: &mut Vec<PathBuf>, p: PathBuf) {
    if !p.as_os_str().is_empty() && !dirs.contains(&p) {
        dirs.push(p);
    }
}

// ------------------------------------------------------------------ settings

/// The player-tunable options that persist across launches — a mirror of the
/// global setting statics plus the engine's master volume. Kept in its own
/// `settings.json` (machine-managed) rather than the hand-editable config.toml.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct SettingsFile {
    #[serde(default)]
    pub text_mode: usize,
    #[serde(default = "default_speed")]
    pub speed: usize,
    #[serde(default)]
    pub theme: usize,
    #[serde(default)]
    pub calib_ms: i64,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "yes")]
    pub prac_left: bool,
    #[serde(default = "yes")]
    pub prac_right: bool,
    #[serde(default = "yes")]
    pub prac_top: bool,
    #[serde(default = "yes")]
    pub prac_home: bool,
    #[serde(default = "yes")]
    pub prac_bottom: bool,
    #[serde(default = "yes")]
    pub prac_punct: bool,
}

fn default_speed() -> usize {
    1 // NORMAL
}
fn default_volume() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

/// Snapshot the live settings (globals + engine) into a serializable struct.
/// Compared frame-to-frame so a write only happens when something changed.
pub fn settings_snapshot(engine: &AudioEngine) -> SettingsFile {
    SettingsFile {
        text_mode: TEXT_MODE_IDX.load(Relaxed),
        speed: SPEED_IDX.load(Relaxed),
        theme: THEME_IDX.load(Relaxed),
        calib_ms: CALIB_MS.load(Relaxed),
        volume: engine.master(),
        prac_left: PRAC_LEFT.load(Relaxed),
        prac_right: PRAC_RIGHT.load(Relaxed),
        prac_top: PRAC_TOP.load(Relaxed),
        prac_home: PRAC_HOME.load(Relaxed),
        prac_bottom: PRAC_BOTTOM.load(Relaxed),
        prac_punct: PRAC_PUNCT.load(Relaxed),
    }
}

/// Read the persisted settings (if any) and apply them to the globals and the
/// engine. Indices are taken modulo their table length, so a file written by a
/// build with more or fewer options can never index out of range.
pub fn load_settings(engine: &AudioEngine) {
    let Some(path) = state_file("settings.json") else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(s) = serde_json::from_str::<SettingsFile>(&text) else { return };
    TEXT_MODE_IDX.store(s.text_mode % TEXT_MODES.len(), Relaxed);
    SPEED_IDX.store(s.speed % SPEEDS.len(), Relaxed);
    THEME_IDX.store(s.theme % THEMES.len(), Relaxed);
    CALIB_MS.store(s.calib_ms.clamp(-500, 500), Relaxed);
    engine.set_master(s.volume);
    PRAC_LEFT.store(s.prac_left, Relaxed);
    PRAC_RIGHT.store(s.prac_right, Relaxed);
    PRAC_TOP.store(s.prac_top, Relaxed);
    PRAC_HOME.store(s.prac_home, Relaxed);
    PRAC_BOTTOM.store(s.prac_bottom, Relaxed);
    PRAC_PUNCT.store(s.prac_punct, Relaxed);
}

/// Persist a settings snapshot. Best-effort: any IO error is swallowed (the
/// in-memory state still works for the session).
pub fn save_settings(s: &SettingsFile) {
    let Some(path) = state_file("settings.json") else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(j) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path, j);
    }
}

// -------------------------------------------------------------------- scores

/// A song+difficulty personal best. `score` is the ranking metric; accuracy
/// and combo ride along for display.
#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct BestScore {
    pub score: i64,
    pub accuracy: f64,
    pub max_combo: i64,
}

/// What `Scores::record` found and did.
pub struct Recorded {
    /// The score previously on file for this song+difficulty, if any.
    pub prev_best: Option<i64>,
    /// Whether the run just recorded became the new best (beat or first-set it).
    pub improved: bool,
}

/// Persisted personal-best scores, keyed by song title + artist + difficulty
/// so they survive files being moved or re-imported.
pub struct Scores {
    map: HashMap<String, BestScore>,
    path: Option<PathBuf>,
}

fn score_key(title: &str, artist: &str, diff: usize, mode: &str) -> String {
    // Unit-separator delimited so ordinary titles/artists can't collide.
    format!("{title}\u{1f}{artist}\u{1f}{diff}\u{1f}{mode}")
}

impl Scores {
    pub fn load() -> Self {
        let path = state_file("scores.json");
        let map = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<HashMap<String, BestScore>>(&s).ok())
            .unwrap_or_default();
        Scores { map, path }
    }

    /// The stored best for a song+difficulty+mode, if one exists.
    pub fn best(&self, title: &str, artist: &str, diff: usize, mode: &str) -> Option<BestScore> {
        self.map.get(&score_key(title, artist, diff, mode)).copied()
    }

    /// Record a finished run, replacing the stored best only when it scores
    /// higher (or none existed). Persists on change.
    pub fn record(
        &mut self,
        title: &str,
        artist: &str,
        diff: usize,
        mode: &str,
        run: BestScore,
    ) -> Recorded {
        let key = score_key(title, artist, diff, mode);
        let prev_best = self.map.get(&key).map(|b| b.score);
        let improved = prev_best.is_none_or(|p| run.score > p);
        if improved {
            self.map.insert(key, run);
            self.save();
        }
        Recorded { prev_best, improved }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(j) = serde_json::to_string_pretty(&self.map) {
            let _ = std::fs::write(path, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_multiline_and_inline() {
        let multiline = "song_dirs = [\n  \"/a/b\",\n  \"/c/d\",\n]\n";
        assert_eq!(parse_song_dirs(multiline), vec![PathBuf::from("/a/b"), PathBuf::from("/c/d")]);
        let inline = "song_dirs = [\"/x\", \"/y\"]";
        assert_eq!(parse_song_dirs(inline), vec![PathBuf::from("/x"), PathBuf::from("/y")]);
    }

    #[test]
    fn ignores_comments_and_empty() {
        let s = "# a comment\nsong_dirs = [\n  # inner\n  \"/only\",\n]\n";
        assert_eq!(parse_song_dirs(s), vec![PathBuf::from("/only")]);
        assert!(parse_song_dirs("song_dirs = []").is_empty());
    }

    #[test]
    fn dedupes_on_push() {
        let mut v = Vec::new();
        push_unique(&mut v, PathBuf::from("/a"));
        push_unique(&mut v, PathBuf::from("/a"));
        push_unique(&mut v, PathBuf::from(""));
        assert_eq!(v, vec![PathBuf::from("/a")]);
    }

    /// A best is set silently on the first clear (no banner), only replaced by
    /// a strictly higher score, and kept per song+artist+difficulty.
    #[test]
    fn scores_record_only_on_improvement() {
        // path: None keeps save() a no-op, so the test never touches disk.
        let mut s = Scores { map: HashMap::new(), path: None };
        let run = |score| BestScore { score, accuracy: 90.0, max_combo: 10 };

        // First clear: improved, but there's no prior score to have beaten.
        let first = s.record("Song", "Artist", 3, "WORDS", run(1000));
        assert!(first.improved && first.prev_best.is_none());
        assert_eq!(s.best("Song", "Artist", 3, "WORDS").map(|b| b.score), Some(1000));

        // A worse (or equal) run doesn't move the record.
        assert!(!s.record("Song", "Artist", 3, "WORDS", run(800)).improved);
        assert!(!s.record("Song", "Artist", 3, "WORDS", run(1000)).improved);
        assert_eq!(s.best("Song", "Artist", 3, "WORDS").map(|b| b.score), Some(1000));

        // A better run improves and reports the score it beat.
        let better = s.record("Song", "Artist", 3, "WORDS", run(1500));
        assert!(better.improved && better.prev_best == Some(1000));
        assert_eq!(s.best("Song", "Artist", 3, "WORDS").map(|b| b.score), Some(1500));

        // Another difficulty of the same song keeps its own record.
        assert!(s.best("Song", "Artist", 2, "WORDS").is_none());

        // A different mode on the same song+difficulty keeps its own record.
        assert!(s.best("Song", "Artist", 3, "DFJK").is_none());
        s.record("Song", "Artist", 3, "DFJK", run(50));
        assert_eq!(s.best("Song", "Artist", 3, "DFJK").map(|b| b.score), Some(50));
        assert_eq!(s.best("Song", "Artist", 3, "WORDS").map(|b| b.score), Some(1500));
    }

    /// The whole point of this module: an extra folder full of .sng files is
    /// scanned alongside the bundled songs/ dir, exactly as main() composes it
    /// (bundled root first, then config.song_dirs). Uses a real bundled .sng
    /// copied into a temp dir so the merge goes through the actual loader.
    #[test]
    fn extra_folder_merges_with_bundled() {
        let bundled = std::path::Path::new("songs");
        let sample = bundled.join("Code Monkey.sng");
        if !sample.exists() {
            return; // running outside the repo checkout
        }
        let bundled_only = crate::chart::scan_all(&[bundled.to_path_buf()]);

        // A fresh temp folder holding one real song, standing in for a user's
        // Clone Hero library elsewhere on disk.
        let extra = std::env::temp_dir().join(format!("kw_extra_{}", std::process::id()));
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::copy(&sample, extra.join("My Imported Song.sng")).unwrap();

        // main() builds roots as [songs/, ...config.song_dirs]; mirror that.
        let roots = vec![bundled.to_path_buf(), extra.clone()];
        let merged = crate::chart::scan_all(&roots);
        assert_eq!(
            merged.len(),
            bundled_only.len() + 1,
            "the extra folder's song joins the bundled library"
        );
        let _ = std::fs::remove_dir_all(&extra);
    }
}
