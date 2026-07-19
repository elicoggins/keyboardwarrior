// In-app Chorus Encore (enchor.us) song search + download. Lets the player find
// a Clone Hero chart and pull it straight into their library without leaving the
// game — the downloaded .sng plays as-is (see sng.rs), no unpacking or rebuild.
//
// The public JSON API used here mirrors what enchor.us's own website does:
//   search:   POST https://api.enchor.us/search  {search, page, ...}
//   download: GET  https://files.enchor.us/{md5}.sng   (direct, no session)
//
// Native only: the browser demo has no filesystem to download into.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

const SEARCH_URL: &str = "https://api.enchor.us/search";
const UA: &str = "KeyboardWarrior/0.1 (+https://github.com/eli-cog/keyboardwarrior)";

/// One search hit — just the fields the menu shows and the download needs.
pub struct Hit {
    pub name: String,
    pub artist: String,
    pub charter: String,
    /// Content hash; the .sng lives at files.enchor.us/{md5}.sng.
    pub md5: String,
    /// Hardest guitar tier the chart carries: 0 Easy … 4 Expert+, or -1 if the
    /// chart has no guitar track.
    pub diff_guitar: i32,
}

/// Guitar difficulty tiers the search can filter on (name, API value). `None`
/// is "any" — no filter. The API only applies a difficulty when an instrument
/// is set, so we always pair this with instrument=guitar.
pub const DIFF_FILTERS: [(&str, Option<&str>); 5] = [
    ("Any", None),
    ("Easy", Some("easy")),
    ("Medium", Some("medium")),
    ("Hard", Some("hard")),
    ("Expert", Some("expert")),
];

// ---- wire types (Chorus response), only the fields we read ---------------
// The API returns hits under `data` (the website then renames it to `songs`
// client-side); each hit carries name/artist/charter, the md5 the .sng download
// is keyed on, and per-instrument difficulty tiers. Verified against the live
// api.enchor.us/search.

#[derive(Deserialize)]
struct SearchResp {
    #[serde(default)]
    data: Vec<RawSong>,
}

#[derive(Deserialize)]
struct RawSong {
    #[serde(default)]
    name: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    charter: String,
    #[serde(default)]
    md5: String,
    // Hardest charted tier per instrument, -1 = absent. We surface guitar.
    #[serde(default = "neg_one")]
    diff_guitar: i32,
}

fn neg_one() -> i32 {
    -1
}

/// Search Chorus for `query` (page is 1-based). `difficulty` is one of the
/// [`DIFF_FILTERS`] API values (e.g. "easy"); when set it's paired with
/// instrument=guitar so the API actually narrows to charts that have a guitar
/// track at that tier. Returns hits that have a usable md5 (a few charts are
/// zip-only and can't be pulled as a .sng — those are dropped so every listed
/// result is downloadable).
pub fn search(query: &str, page: u32, difficulty: Option<&str>) -> Result<Vec<Hit>, String> {
    // A difficulty filter only bites when an instrument is set; guitar is what
    // this game plays, so scope the filter to guitar.
    let instrument = difficulty.map(|_| "guitar");
    let body = ureq::json!({
        "search": query,
        "page": page,
        "instrument": instrument,
        "difficulty": difficulty,
        "drumType": null,
        "drumsReviewed": false,
        "source": "website",
    });
    let resp = ureq::post(SEARCH_URL)
        .timeout(Duration::from_secs(15))
        .set("User-Agent", UA)
        .set("Origin", "https://www.enchor.us")
        .set("Referer", "https://www.enchor.us/")
        .send_json(body)
        .map_err(|e| net_error("search", e))?;
    let parsed: SearchResp =
        resp.into_json().map_err(|e| format!("couldn't read search results: {e}"))?;
    let hits = parsed
        .data
        .into_iter()
        .filter(|s| !s.md5.is_empty())
        .map(|s| Hit {
            name: s.name,
            artist: s.artist,
            charter: s.charter,
            md5: s.md5,
            diff_guitar: s.diff_guitar,
        })
        .collect();
    Ok(hits)
}

/// Download the hit's .sng into `dest_dir`, named "Artist - Title.sng" (falling
/// back to the md5 if both are blank). Returns the written path.
pub fn download(hit: &Hit, dest_dir: &Path) -> Result<PathBuf, String> {
    let url = format!("https://files.enchor.us/{}.sng", hit.md5);
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(60))
        .set("User-Agent", UA)
        .set("Referer", "https://www.enchor.us/")
        .call()
        .map_err(|e| net_error("download", e))?;

    let mut bytes = Vec::new();
    std::io::copy(&mut resp.into_reader(), &mut bytes)
        .map_err(|e| format!("download interrupted: {e}"))?;
    if bytes.is_empty() {
        return Err("download was empty".into());
    }

    std::fs::create_dir_all(dest_dir).map_err(|e| format!("can't write to songs folder: {e}"))?;
    let base = file_stem(hit);
    let path = unique_path(dest_dir, &base);
    std::fs::write(&path, &bytes).map_err(|e| format!("couldn't save song: {e}"))?;
    Ok(path)
}

/// A filesystem-safe "Artist - Title" (or md5 if unnamed), no extension.
fn file_stem(hit: &Hit) -> String {
    let raw = match (hit.artist.trim(), hit.name.trim()) {
        ("", "") => hit.md5.clone(),
        ("", n) => n.to_string(),
        (a, "") => a.to_string(),
        (a, n) => format!("{a} - {n}"),
    };
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        hit.md5.clone()
    } else {
        trimmed.to_string()
    }
}

/// `{base}.sng`, or `{base} (2).sng`, … so re-downloading never clobbers an
/// existing file.
fn unique_path(dir: &Path, base: &str) -> PathBuf {
    let first = dir.join(format!("{base}.sng"));
    if !first.exists() {
        return first;
    }
    for n in 2.. {
        let p = dir.join(format!("{base} ({n}).sng"));
        if !p.exists() {
            return p;
        }
    }
    unreachable!("ran out of integers")
}

/// Turn a ureq error into a short, player-facing message. HTTP status codes get
/// their own line so a 404 (chart withdrawn) reads differently from no network.
fn net_error(action: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("{action} failed (server said {code})"),
        ureq::Error::Transport(t) => format!("{action} failed — check your connection ({t})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live end-to-end against enchor.us — search returns hits with md5s, and
    /// the top hit's .sng downloads as a valid SNGPKG container. Ignored by
    /// default (network); run with `cargo test -- --ignored chorus`.
    #[test]
    #[ignore]
    fn live_search_and_download() {
        let hits = search("code monkey jonathan coulton", 1, None).expect("search should succeed");
        assert!(!hits.is_empty(), "expected at least one hit");
        assert!(hits.iter().all(|h| !h.md5.is_empty()));

        // The difficulty filter narrows to guitar charts at that tier.
        let easy = search("nirvana", 1, Some("easy")).expect("filtered search");
        let all = search("nirvana", 1, None).expect("unfiltered search");
        assert!(easy.len() <= all.len(), "filter should not widen results");
        let dir = std::env::temp_dir().join("kw_chorus_test");
        let path = download(&hits[0], &dir).expect("download should succeed");
        let head = std::fs::read(&path).expect("saved file readable");
        assert_eq!(&head[..6], b"SNGPKG", "downloaded a real .sng container");
        let _ = std::fs::remove_file(&path);
    }
}
