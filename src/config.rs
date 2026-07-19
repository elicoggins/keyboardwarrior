// On-disk player config — the app's first persisted state. Right now it holds
// just the list of *extra* song folders the player has pointed the game at
// (e.g. an existing Clone Hero library elsewhere on disk). The bundled songs/
// dir is always scanned on top of these; extra folders are additive, never a
// replacement.
//
// Native only: the browser demo has no filesystem and ships a fixed library.

use std::path::PathBuf;

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
        let (bundled_only, _) = crate::chart::scan_all(&[bundled.to_path_buf()]);

        // A fresh temp folder holding one real song, standing in for a user's
        // Clone Hero library elsewhere on disk.
        let extra = std::env::temp_dir().join(format!("kw_extra_{}", std::process::id()));
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::copy(&sample, extra.join("My Imported Song.sng")).unwrap();

        // main() builds roots as [songs/, ...config.song_dirs]; mirror that.
        let roots = vec![bundled.to_path_buf(), extra.clone()];
        let (merged, _) = crate::chart::scan_all(&roots);
        assert_eq!(
            merged.len(),
            bundled_only.len() + 1,
            "the extra folder's song joins the bundled library"
        );
        let _ = std::fs::remove_dir_all(&extra);
    }
}
