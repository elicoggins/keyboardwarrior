// Keyboard Warrior — a rhythm typing game.
// Guitar-hero note highway where every gem is a letter; lanes map to finger
// zones on a QWERTY keyboard. Plays real Clone Hero songs (.sng or folders)
// through its own cpal mixer, whose sample counter IS the game clock.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

use macroquad::prelude::*;

mod audio;
mod chart;
mod decode;
mod sng;

use audio::{AudioEngine, Buf};
use chart::{SongChart, SongSource, DIFF_NAMES};

// Timing windows (seconds from the note's ideal time)
const PERFECT_WIN: f64 = 0.055;
const GREAT_WIN: f64 = 0.110;
const GOOD_WIN: f64 = 0.170;
// How long a note is visible before it reaches the strike line
const APPROACH: f64 = 1.5;
// Calibration metronome period (120 BPM)
const CALIB_PERIOD: f64 = 0.5;

// Input latency compensation measured on the calibration screen, in ms.
// Subtracted from the clock when judging keypresses (visuals stay raw).
static CALIB_MS: AtomicI64 = AtomicI64::new(0);

fn calib_offset() -> f64 {
    CALIB_MS.load(Ordering::Relaxed) as f64 / 1000.0
}

/// Mini keyboard legend drawn in the menu: every key tinted by its lane, so
/// the lane-to-hand mapping is shown, not spelled out.
fn draw_keyboard_legend(center_x: f32, top_y: f32) {
    let rows: [(&str, f32); 3] = [("qwertyuiop", 0.0), ("asdfghjkl", 0.4), ("zxcvbnm", 1.0)];
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
            let d = msize(&label, 13);
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

// ---------------------------------------------------------------- themes

struct Theme {
    name: &'static str,
    bg: Color,
    lane: [Color; 4],
    accent: Color,    // star power, combo multiplier, highlights
    secondary: Color, // subtitles, progress, GREAT judgement
    good: Color,      // GOOD judgement
    miss: Color,
}

const THEMES: [Theme; 3] = [
    // Black / slate / orange: near-black base, two ember lanes, two slate
    Theme {
        name: "EMBER",
        bg: Color::new(0.043, 0.045, 0.052, 1.0),
        lane: [
            Color::new(0.96, 0.62, 0.12, 1.0), // amber
            Color::new(0.98, 0.45, 0.10, 1.0), // orange
            Color::new(0.34, 0.48, 0.72, 1.0), // steel blue
            Color::new(0.85, 0.88, 0.92, 1.0), // pale slate
        ],
        accent: Color::new(0.99, 0.72, 0.25, 1.0),
        secondary: Color::new(0.64, 0.70, 0.78, 1.0),
        good: Color::new(0.72, 0.70, 0.66, 1.0),
        miss: Color::new(0.94, 0.33, 0.25, 1.0),
    },
    // Deep indigo night with jewel-tone lanes
    Theme {
        name: "MIDNIGHT",
        bg: Color::new(0.055, 0.058, 0.098, 1.0),
        lane: [
            Color::new(0.18, 0.83, 0.75, 1.0), // teal
            Color::new(0.65, 0.55, 0.98, 1.0), // violet
            Color::new(0.49, 0.83, 0.99, 1.0), // sky
            Color::new(0.98, 0.44, 0.52, 1.0), // rose
        ],
        accent: Color::new(0.99, 0.83, 0.30, 1.0),
        secondary: Color::new(0.45, 0.80, 1.00, 1.0),
        good: Color::new(0.75, 0.75, 0.80, 1.0),
        miss: Color::new(1.00, 0.33, 0.33, 1.0),
    },
    // Dark evergreen with northern-lights lanes
    Theme {
        name: "AURORA",
        bg: Color::new(0.035, 0.062, 0.055, 1.0),
        lane: [
            Color::new(0.43, 0.91, 0.72, 1.0), // mint
            Color::new(0.40, 0.88, 0.98, 1.0), // cyan
            Color::new(0.77, 0.71, 0.99, 1.0), // lilac
            Color::new(0.99, 0.86, 0.55, 1.0), // sand
        ],
        accent: Color::new(0.96, 0.78, 0.42, 1.0),
        secondary: Color::new(0.45, 0.86, 0.83, 1.0),
        good: Color::new(0.70, 0.78, 0.75, 1.0),
        miss: Color::new(1.00, 0.42, 0.42, 1.0),
    },
];

static THEME_IDX: AtomicUsize = AtomicUsize::new(0);
static SENTENCE_MODE: AtomicBool = AtomicBool::new(false);

fn th() -> &'static Theme {
    &THEMES[THEME_IDX.load(Ordering::Relaxed) % THEMES.len()]
}

fn sentence_mode() -> bool {
    SENTENCE_MODE.load(Ordering::Relaxed)
}

/// A theme color at a given alpha.
fn wa(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Blend two colors.
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

// ---------------------------------------------------------------- typography

// The built-in pixel font, everywhere — menu, HUD, gems, words.
fn dtext(t: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(t, x, y, size, color);
}

fn msize(t: &str, size: u16) -> TextDimensions {
    measure_text(t, None, size, 1.0)
}

// Word pools indexed by length - 1. In WORDS mode a phrase with N notes gets
// a word with N letters, so typing a word IS playing a lick. Pools are large
// and dealt from a reshuffling deck, so repeats are rare and never adjacent.
const WORDS_BY_LEN: [&[&str]; 8] = [
    &["a", "i"],
    &[
        "go", "up", "on", "we", "it", "my", "do", "so", "am", "an", "as", "at", "be", "by", "he",
        "if", "in", "is", "me", "no", "of", "or", "to", "us",
    ],
    &[
        "the", "and", "for", "you", "not", "are", "all", "new", "was", "can", "has", "but", "our",
        "one", "may", "out", "use", "any", "see", "his", "who", "web", "now", "get", "how", "its",
        "top", "had", "day", "two", "buy", "her", "add", "she", "set", "map", "way", "off", "did",
        "car", "own", "end", "him", "per", "big", "law", "art", "old", "non", "why", "low", "man",
        "job", "too", "men", "box", "gay", "air", "yes", "hot", "say", "san", "tax", "got", "let",
        "act", "red", "key", "few", "age", "pay", "war", "fax", "yet", "rss", "run", "put", "try",
        "log", "fun", "lot", "ask", "due", "pro", "ago", "via", "bad", "far", "oil", "bit", "bay",
        "bar", "dog", "usr", "gas", "six", "pre", "zip", "bid", "inn", "los", "win", "bed", "sea",
        "cut", "tel", "kit", "boy", "son", "mac", "bin", "van", "ads", "pop", "hit", "eye", "fee",
        "las", "aid", "fat", "saw", "tom", "led", "fan", "ten", "cat", "die", "pet", "guy", "dev",
        "cup", "lee", "bob", "fit", "met", "ice", "sec", "bus", "bag", "ibm",
    ],
    &[
        "that", "this", "with", "from", "your", "have", "more", "will", "home", "page", "free",
        "time", "they", "site", "what", "news", "only", "when", "here", "also", "help", "view",
        "been", "were", "some", "like", "than", "find", "date", "back", "list", "name", "just",
        "over", "year", "into", "next", "used", "work", "last", "most", "data", "make", "them",
        "post", "city", "such", "best", "then", "good", "well", "info", "high", "each", "very",
        "book", "read", "need", "many", "user", "said", "does", "mail", "full", "life", "know",
        "days", "part", "real", "item", "must", "made", "line", "send", "type", "take", "area",
        "want", "long", "code", "show", "even", "much", "sign", "file", "link", "open", "case",
        "same", "both", "game", "care", "down", "size", "shop", "text", "rate", "form", "love",
        "john", "main", "call", "save", "york", "card", "jobs", "food", "sale", "teen", "room",
        "join", "west", "look", "left", "team", "week", "note", "live", "june", "plan", "cost",
        "july", "test", "come", "cart", "play", "less", "blog", "park", "side", "give", "sell",
        "body", "east", "club", "road", "gift", "hard", "four", "blue", "easy", "star", "hand",
        "keep", "baby", "term", "film", "head", "cell", "self", "away", "once", "sure", "cars",
        "tell", "able", "gold", "arts", "past", "five", "upon", "says", "land", "done", "ever",
        "word", "bill", "talk", "kids", "true", "else", "mark", "rock", "tips", "plus", "auto",
        "edit", "fast", "fact", "unit", "tech", "meet", "feel", "bank", "risk", "town", "girl",
        "toys", "golf", "loan", "wide", "sort", "half", "step", "none", "paul", "lake", "fire",
        "chat", "loss",
    ],
    &[
        "about", "other", "which", "their", "there", "first", "would", "these", "click", "price",
        "state", "world", "music", "after", "video", "where", "books", "links", "years", "order",
        "items", "group", "under", "games", "could", "great", "hotel", "store", "terms", "right",
        "local", "those", "using", "phone", "forum", "based", "black", "check", "index", "being",
        "women", "today", "south", "pages", "found", "house", "photo", "power", "while", "three",
        "total", "place", "think", "north", "posts", "media", "water", "since", "guide", "board",
        "white", "small", "times", "sites", "level", "hours", "image", "title", "shall", "class",
        "still", "money", "every", "visit", "tools", "reply", "value", "press", "learn", "print",
        "stock", "point", "sales", "large", "table", "start", "model", "human", "movie", "march",
        "going", "study", "staff", "again", "april", "never", "users", "topic", "below", "party",
        "legal", "above", "quote", "story", "rates", "young", "field", "paper", "girls", "night",
        "texas", "poker", "issue", "range", "court", "audio", "light", "write", "offer", "given",
        "files", "event", "china", "needs", "might", "month", "major", "areas", "space", "cards",
        "child", "enter", "share", "added", "radio", "until", "color", "track", "least", "trade",
        "david", "green", "close", "drive", "short", "means", "daily", "beach", "costs", "style",
        "front", "parts", "early", "miles", "sound", "works", "rules", "final", "adult", "thing",
        "cheap", "third", "gifts", "cover", "often", "watch", "deals", "words", "james", "heart",
        "error", "clear", "makes", "india", "taken", "known", "cases", "quick", "whole", "later",
        "basic", "shows", "along", "among", "death", "speed", "brand", "stuff", "japan", "doing",
        "loans", "shoes", "entry", "notes", "force", "river", "album", "views", "plans", "build",
    ],
    &[
        "search", "people", "health", "should", "system", "policy", "number", "please", "rights",
        "public", "school", "review", "united", "center", "travel", "report", "member", "before",
        "hotels", "office", "design", "posted", "within", "states", "family", "prices", "sports",
        "county", "access", "change", "rating", "during", "return", "events", "little", "movies",
        "source", "author", "around", "course", "canada", "credit", "estate", "select", "photos",
        "thread", "market", "really", "action", "series", "second", "forums", "better", "friend",
        "issues", "street", "things", "person", "mobile", "offers", "recent", "stores", "memory",
        "social", "august", "create", "single", "latest", "status", "browse", "seller", "always",
        "result", "groups", "making", "future", "london", "become", "garden", "listed", "energy",
        "images", "notice", "others", "format", "months", "safety", "having", "common", "living",
        "called", "period", "window", "france", "region", "island", "record", "direct", "update",
        "either", "centre", "europe", "topics", "videos", "global", "player", "lyrics", "submit",
        "amount", "though", "thanks", "weight", "choose", "points", "camera", "domain", "beauty",
        "models", "simple", "friday", "annual", "church", "method", "active", "figure", "enough",
        "higher", "yellow", "french", "nature", "orders", "africa", "growth", "agency", "monday",
        "income", "engine", "double", "screen", "across", "needed", "season", "effect", "sunday",
        "casino", "volume", "anyone", "silver", "inside", "mature", "rather", "supply", "robert",
        "skills", "advice", "career", "rental", "middle", "taking", "values", "coming", "object",
        "length", "client", "follow", "sample", "george", "choice", "artist", "levels", "letter",
        "phones", "summer", "degree", "button", "matter", "custom", "almost", "editor", "female",
    ],
    &[
        "contact", "service", "product", "support", "message", "through", "privacy", "company",
        "general", "january", "reviews", "program", "details", "because", "results", "address",
        "subject", "between", "special", "project", "version", "section", "related", "members",
        "network", "systems", "without", "current", "control", "history", "account", "digital",
        "profile", "another", "quality", "listing", "content", "country", "private", "compare",
        "include", "college", "article", "provide", "process", "science", "english", "gallery",
        "however", "october", "library", "medical", "looking", "comment", "working", "against",
        "payment", "student", "problem", "options", "america", "example", "changes", "release",
        "request", "picture", "meeting", "similar", "schools", "million", "popular", "stories",
        "journal", "reports", "welcome", "central", "council", "archive", "society", "friends",
        "edition", "further", "updated", "already", "studies", "several", "display", "limited",
        "powered", "natural", "whether", "weather", "average", "records", "present", "written",
        "federal", "hosting", "tickets", "finance", "minutes", "reading", "usually", "percent",
        "getting", "germany", "various", "receive", "methods", "chapter", "manager", "michael",
        "florida", "license", "holiday", "writing", "effects", "created", "kingdom", "thought",
        "storage", "summary", "western", "overall", "package", "players", "started", "someone",
        "printer", "believe", "nothing", "certain", "running", "jewelry", "islands", "british",
        "sellers", "tuesday", "lesbian", "machine",
    ],
    &[
        "business", "services", "products", "research", "comments", "national", "shipping",
        "reserved", "security", "american", "computer", "download", "pictures", "personal",
        "location", "children", "students", "shopping", "previous", "property", "customer",
        "december", "training", "advanced", "category", "register", "november", "features",
        "industry", "provided", "required", "articles", "feedback", "complete", "standard",
        "programs", "language", "question", "building", "february", "analysis", "possible",
        "problems", "interest", "learning", "delivery", "original", "includes", "messages",
        "provides", "specific", "director", "planning", "official", "district", "calendar",
        "resource", "document", "material", "together", "function", "economic", "projects",
        "included", "received", "archives", "magazine", "policies", "position", "listings",
        "wireless", "purchase", "response", "practice", "designed", "discount", "remember",
        "increase", "european", "activity", "although", "contents", "regional", "supplies",
        "exchange", "continue", "benefits", "anything", "mortgage", "solution", "addition",
        "clothing", "homepage", "military", "decision", "division", "actually", "saturday",
        "starting", "thursday", "consumer", "contract", "releases", "virginia", "multiple",
        "featured", "friendly", "schedule", "everyone", "approach",
    ],
];

// Sentence corpus for SENTENCES mode: coherent text streamed letter-by-letter
// across the chart, monkeytype quote-style. Lowercase a–z plus unshifted
// punctuation (comma, period, apostrophe) — every character is a gem.
const SENTENCES: &[&str] = &[
    "the quick brown fox jumps over the lazy dog.",
    "pack my box with five dozen liquor jugs.",
    "a journey of a thousand miles begins with a single step.",
    "all that glitters is not gold.",
    "the only way out is through.",
    "music is the space between the notes.",
    "fortune favors the bold.",
    "practice doesn't make perfect, it makes permanent.",
    "the stars look very different today.",
    "we're all in the gutter, but some of us are looking at the stars.",
    "type like the wind, and land on the beat.",
    "every wall is a door.",
    "simplicity is the ultimate sophistication.",
    "stay hungry, stay foolish.",
    "the night is young, and so are we.",
    "lightning never strikes the same place twice.",
    "still waters run deep.",
    "the early bird catches the worm.",
    "no rain, no flowers.",
    "what we think, we become.",
    "dance first, think later.",
    "well begun is half done.",
    "action is the foundational key to all success.",
    "creativity takes courage.",
    "leap, and the net will appear.",
    "slow is smooth, and smooth is fast.",
    "the best way to predict the future is to invent it.",
    "where words fail, music speaks.",
    "the rhythm of the night carries us home.",
    "it's not the years in your life, it's the life in your years.",
    "don't wait for opportunity, create it.",
    "small steps every day add up to great distances.",
];

fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = macroquad::rand::gen_range(0usize, i + 1).min(i);
        v.swap(i, j);
    }
}

/// A reshuffling deck of words: never repeats until the pool is exhausted,
/// then reshuffles so even the wraparound order differs.
struct WordDeck {
    words: Vec<&'static str>,
    cursor: usize,
}

impl WordDeck {
    fn new(pool: &'static [&'static str]) -> Self {
        let mut words = pool.to_vec();
        shuffle(&mut words);
        WordDeck { words, cursor: 0 }
    }

    fn next(&mut self) -> &'static str {
        if self.cursor >= self.words.len() {
            shuffle(&mut self.words);
            self.cursor = 0;
        }
        let w = self.words[self.cursor];
        self.cursor += 1;
        w
    }
}

/// Generate the text for a run. `groups` are the phrase sizes (note counts).
/// WORDS mode: one length-matched word per phrase. SENTENCES mode: coherent
/// sentences streamed across the same total number of notes.
fn generate_text(groups: &[usize]) -> Vec<String> {
    let mut decks: Vec<WordDeck> = WORDS_BY_LEN.iter().map(|p| WordDeck::new(p)).collect();
    if !sentence_mode() {
        return groups
            .iter()
            .map(|&len| {
                let idx = (len - 1).min(WORDS_BY_LEN.len() - 1);
                decks[idx].next().to_string()
            })
            .collect();
    }

    // Sentences: deal whole sentences until the letter budget is spent,
    // topping off the tail with an exact-length word so every note has a letter
    let total: usize = groups.iter().sum();
    let mut order: Vec<usize> = (0..SENTENCES.len()).collect();
    shuffle(&mut order);
    let mut words: Vec<String> = Vec::new();
    let mut remaining = total;
    let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::new();
    let mut si = 0;
    while remaining > 0 {
        if queue.is_empty() {
            queue.extend(SENTENCES[order[si % order.len()]].split_whitespace());
            si += 1;
        }
        let w = *queue.front().unwrap();
        if w.len() <= remaining {
            words.push(w.to_string());
            remaining -= w.len();
            queue.pop_front();
        } else if remaining <= WORDS_BY_LEN.len() {
            words.push(decks[remaining - 1].next().to_string());
            remaining = 0;
        } else {
            queue.pop_front(); // word too long for the tail; try the next
        }
    }
    words
}

#[derive(Clone, Copy, PartialEq)]
enum Judgement {
    Perfect,
    Great,
    Good,
}

impl Judgement {
    fn label(self) -> &'static str {
        match self {
            Judgement::Perfect => "PERFECT",
            Judgement::Great => "GREAT",
            Judgement::Good => "GOOD",
        }
    }
    fn color(self) -> Color {
        match self {
            Judgement::Perfect => th().accent,
            Judgement::Great => th().secondary,
            Judgement::Good => th().good,
        }
    }
    fn score(self) -> i64 {
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
    time: f64, // song time in seconds when it should be typed
    word: usize,
    sp_phrase: Option<u16>, // star power phrase this note belongs to
    state: NoteState,
}

fn lane_of(c: char) -> usize {
    match c {
        'q' | 'w' | 'a' | 's' | 'z' | 'x' => 0,
        'e' | 'r' | 't' | 'd' | 'f' | 'g' | 'c' | 'v' | 'b' => 1,
        'y' | 'u' | 'h' | 'j' | 'n' | 'm' => 2,
        // i/k/o/l/p plus the unshifted punctuation keys , . ' — right outer
        _ => 3,
    }
}

/// Characters that can appear on gems: letters plus unshifted punctuation.
fn is_typeable(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, ',' | '.' | '\'')
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

struct Sounds {
    kick: Buf,
    hat: Buf,
    miss: Buf,
}

// ---------------------------------------------------------------- audio synth

/// Synthesize a mono waveform into a device-rate stereo buffer.
fn synth(rate: u32, dur: f32, f: impl Fn(f32) -> f32) -> Buf {
    let n = (dur * rate as f32) as usize;
    Arc::new(
        (0..n)
            .map(|i| {
                let s = f(i as f32 / rate as f32).clamp(-1.0, 1.0);
                [s, s]
            })
            .collect(),
    )
}

fn make_sounds(rate: u32) -> Sounds {
    use std::f32::consts::TAU;
    let noise_cell = std::cell::Cell::new(0x1234_5678u32);
    let noise = || {
        let mut s = noise_cell.get();
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        noise_cell.set(s);
        (s as f32 / u32::MAX as f32) * 2.0 - 1.0
    };

    // Kick: pitch sweep 160 -> 45 Hz with fast decay
    let kick = synth(rate, 0.20, |t| {
        let freq = 45.0 + 115.0 * (-t * 22.0).exp();
        (TAU * freq * t).sin() * (-t * 16.0).exp() * 0.9
    });
    // Hi-hat: short noise burst (crude high-pass via previous-sample subtract)
    let prev = std::cell::Cell::new(0.0f32);
    let hat = synth(rate, 0.05, |t| {
        let raw = noise();
        let hp = raw - prev.get() * 0.7;
        prev.set(raw);
        hp * (-t * 90.0).exp() * 0.30
    });
    // Miss: low buzzy thud
    let miss = synth(rate, 0.22, |t| {
        let square = if (TAU * 108.0 * t).sin() > 0.0 { 1.0 } else { -1.0 };
        square * (-t * 14.0).exp() * 0.30
    });

    Sounds { kick, hat, miss }
}

// ---------------------------------------------------------------- play state

// Which song was played, for the results screen and instant restarts
#[derive(Clone, Copy)]
struct SongRef {
    song: usize, // index into the scanned song list
    diff: usize, // chart difficulty
}

struct SpPhrase {
    len: usize,
    hits: usize,
    broken: bool,
}

struct Play {
    song_ref: SongRef,
    title: String,
    diff_name: String,
    notes: Vec<Note>,
    words: Vec<String>,
    // First note that could still be Pending; hit/missed prefixes are never
    // rescanned, so keypress matching stays O(window) on long charts
    cursor: usize,
    // notes index where each word starts (notes are sorted, words contiguous)
    word_starts: Vec<usize>,
    paused: bool,
    pause_now: f64, // clock value frozen at the moment of pausing, for draw
    ducked: bool,   // lead stem is currently ducked after a miss
    beats: Vec<f64>,
    next_beat: usize,
    sp_phrases: Vec<SpPhrase>,
    energy: f32,
    sp_until: f64,
    word_anim: f32, // eased index of the current word, drives the word queue
    spb: f64,
    score: i64,
    combo: i64,
    max_combo: i64,
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
    strays: u32,
    particles: Vec<Particle>,
    floaters: Vec<Floater>,
    shake: f32,
    beat_flash: f32,
    first_note_time: f64,
    end_time: f64,
}

struct Geom {
    left: f32,
    width: f32,
    lane_w: f32,
    hit_y: f32,
    top: f32,
}

fn geom() -> Geom {
    let w = screen_width();
    let h = screen_height();
    let width = (w * 0.62).min(720.0);
    let left = (w - width) / 2.0;
    Geom { left, width, lane_w: width / 4.0, hit_y: h * 0.78, top: 70.0 }
}

/// Stream the text's letters onto note times in order: letter k of the text
/// rides note k. Word boundaries drive the on-screen word queue.
fn assign_letters(words: &[String], times: &[f64]) -> Vec<Note> {
    let mut notes = Vec::with_capacity(times.len());
    let (mut wi, mut li) = (0usize, 0usize);
    for &t in times.iter() {
        while wi < words.len() && li >= words[wi].len() {
            wi += 1;
            li = 0;
        }
        let Some(word) = words.get(wi) else { break };
        let ch = word.as_bytes()[li] as char;
        li += 1;
        notes.push(Note {
            ch,
            lane: lane_of(ch),
            time: t,
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
    fn new_chart(
        song_idx: usize,
        diff: usize,
        chart: &SongChart,
        engine: &AudioEngine,
        snd: &Sounds,
        backing: Buf,
        lead: Option<Buf>,
    ) -> Self {
        let times: Vec<f64> = chart.diffs[diff].iter().map(|n| n.time).collect();

        // Group notes into phrases at musical rests (or when a word maxes out)
        let mut groups: Vec<Vec<f64>> = Vec::new();
        for &t in &times {
            let new_group = match groups.last().and_then(|g| g.last()) {
                Some(&prev) => t - prev > 0.85 || groups.last().unwrap().len() >= 8,
                None => true,
            };
            if new_group {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push(t);
        }
        // Fold lonely single-note groups into the previous word when close
        let mut merged: Vec<Vec<f64>> = Vec::new();
        for g in groups {
            match merged.last_mut() {
                Some(prev)
                    if g.len() == 1 && prev.len() < 8 && g[0] - *prev.last().unwrap() < 1.6 =>
                {
                    prev.extend(g);
                }
                _ => merged.push(g),
            }
        }

        let group_lens: Vec<usize> = merged.iter().map(|g| g.len()).collect();
        let flat_times: Vec<f64> = merged.concat();
        let words = generate_text(&group_lens);
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

        let spb = if chart.beats.len() > 1 { chart.beats[1] - chart.beats[0] } else { 0.5 };
        let first = notes.first().map_or(0.0, |n| n.time);
        let end_time = chart.end + 3.0;
        // Three-second lead-in; the stems begin at exactly timeline zero,
        // with four tempo-matched count-in ticks scheduled sample-exact
        engine.start_timeline(3.0, Some(backing), lead);
        for i in 1..=4 {
            engine.play_at(&snd.hat, 0.6, -(i as f64) * spb);
        }
        Self::from_parts(
            SongRef { song: song_idx, diff },
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
            paused: false,
            pause_now: 0.0,
            ducked: false,
            beats,
            next_beat: 0,
            sp_phrases,
            energy: 0.0,
            sp_until: -1.0,
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
            beat_flash: 0.0,
            first_note_time,
            end_time,
        }
    }

    fn sp_active(&self, now: f64) -> bool {
        now < self.sp_until
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
        let x = g.left + g.lane_w * (note.lane as f32 + 0.5);
        let progress = ((note.time - now) / APPROACH) as f32;
        let y = g.hit_y - progress * (g.hit_y - g.top);
        vec2(x, y)
    }

    fn burst(&mut self, pos: Vec2, color: Color, count: usize) {
        for _ in 0..count {
            let ang = macroquad::rand::gen_range(0.0f32, std::f32::consts::TAU);
            let speed = macroquad::rand::gen_range(60.0f32, 380.0);
            let life = macroquad::rand::gen_range(0.25f32, 0.6);
            self.particles.push(Particle {
                pos,
                vel: vec2(ang.cos(), ang.sin()) * speed,
                life,
                max_life: life,
                size: macroquad::rand::gen_range(2.0f32, 5.5),
                color,
            });
        }
    }

    fn float_text(&mut self, text: &str, pos: Vec2, color: Color, size: f32) {
        self.floaters.push(Floater { text: text.to_string(), pos, life: 0.8, color, size });
    }

    fn handle_char(&mut self, c: char, now: f64, snd: &Sounds, engine: &AudioEngine) {
        // Space deploys banked star power: 2x score while it lasts
        if c == ' ' {
            if self.energy >= 0.5 && !self.sp_active(now) {
                self.sp_until = now + self.energy as f64 * 16.0;
                self.energy = 0.0;
                let g = geom();
                self.float_text(
                    "STAR POWER!",
                    vec2(g.left + g.width / 2.0, g.hit_y - 130.0),
                    wa(th().accent, 1.0),
                    44.0,
                );
                engine.play(&snd.kick, 0.6);
            }
            return;
        }
        let c = c.to_ascii_lowercase();
        if !is_typeable(c) {
            return;
        }
        if now < self.first_note_time - GOOD_WIN {
            return; // still in the count-in
        }
        let g = geom();

        self.advance_cursor();
        let mut best: Option<(usize, f64)> = None;
        for i in self.cursor..self.notes.len() {
            let n = &self.notes[i];
            if n.time - now > GOOD_WIN {
                break; // notes are sorted by time
            }
            if n.state != NoteState::Pending || n.ch != c {
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
                // Star power phrase progress: complete a phrase cleanly to
                // bank energy
                if let Some(p) = self.notes[i].sp_phrase {
                    let ph = &mut self.sp_phrases[p as usize];
                    ph.hits += 1;
                    if !ph.broken && ph.hits == ph.len {
                        self.energy = (self.energy + 0.25).min(1.0);
                        let g2 = geom();
                        self.float_text(
                            "STAR POWER +",
                            vec2(g2.left + g2.width / 2.0, g2.hit_y - 100.0),
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
                self.float_text(j.label(), vec2(pos.x, g.hit_y - 64.0), j.color(), 26.0);
                // A clean hit brings the ducked lead stem back into the mix
                if self.ducked {
                    engine.set_lead_gain(1.0);
                    self.ducked = false;
                }
            }
            None => {
                // Stray keypress: no matching note in the window
                self.strays += 1;
                self.combo = 0;
                self.shake = self.shake.max(3.0);
                self.float_text("X", vec2(g.left + g.width / 2.0, g.hit_y - 40.0), th().miss, 24.0);
                engine.play(&snd.miss, 0.18);
            }
        }
    }

    /// `now` is the raw clock (visuals); `jnow` has the calibration offset
    /// applied and drives everything that judges the player.
    fn update(&mut self, now: f64, jnow: f64, snd: &Sounds, engine: &AudioEngine) {
        let dt = get_frame_time();
        let g = geom();

        // Visual pulse on each beat (beat times come from the tempo map)
        while self.next_beat < self.beats.len() && self.beats[self.next_beat] <= now {
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
            self.shake = self.shake.max(7.0);
            if let Some(p) = self.notes[i].sp_phrase {
                self.sp_phrases[p as usize].broken = true;
            }
            let x = g.left + g.lane_w * (self.notes[i].lane as f32 + 0.5);
            self.float_text("MISS", vec2(x, g.hit_y - 64.0), wa(th().miss, 1.0), 26.0);
            engine.play(&snd.miss, 0.35);
            // Fumbling the line makes the lead drop out of the mix
            if !self.ducked {
                engine.set_lead_gain(0.12);
                self.ducked = true;
            }
        }

        // Effects
        for p in self.particles.iter_mut() {
            p.pos += p.vel * dt;
            p.vel *= 1.0 - 2.5 * dt;
            p.vel.y += 300.0 * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for f in self.floaters.iter_mut() {
            f.pos.y -= 55.0 * dt;
            f.life -= dt;
        }
        self.floaters.retain(|f| f.life > 0.0);
        self.shake = (self.shake - 30.0 * dt).max(0.0);
        self.beat_flash = (self.beat_flash - 4.0 * dt).max(0.0);

        // Ease the word queue toward the current word (completed words slide
        // up and out, upcoming words rise into place). After advance_cursor
        // the cursor sits on the first pending note.
        self.advance_cursor();
        let target = self.notes.get(self.cursor).map(|n| n.word).unwrap_or(self.words.len()) as f32;
        let k = 1.0 - (-dt * 9.0).exp();
        self.word_anim += (target - self.word_anim) * k;
    }

    fn finished(&self, now: f64) -> bool {
        now > self.end_time + 1.0
    }

    fn hits(&self) -> u32 {
        self.perfect + self.great + self.good
    }

    fn accuracy(&self) -> f64 {
        let total = self.hits() + self.miss + self.strays;
        if total == 0 {
            return 100.0;
        }
        self.hits() as f64 / total as f64 * 100.0
    }

    // ------------------------------------------------------------ rendering

    fn draw(&self, now: f64) {
        let g = geom();
        let h = screen_height();

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
            if t > now + APPROACH {
                break;
            }
            let progress = ((t - now) / APPROACH) as f32;
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

        // Strike line
        let flash = 0.42 + 0.14 * self.beat_flash;
        draw_line(
            g.left + ox,
            g.hit_y + oy,
            g.left + g.width + ox,
            g.hit_y + oy,
            4.0,
            Color::new(1.0, 1.0, 1.0, flash),
        );
        for lane in 0..4 {
            let x = g.left + g.lane_w * (lane as f32 + 0.5) + ox;
            let mut c = th().lane[lane];
            c.a = 0.30 + 0.12 * self.beat_flash;
            draw_circle_lines(x, g.hit_y + oy, 24.0, 2.0, c);
        }

        // Only notes near the highway need drawing: anything more than one
        // approach-window old is far below the screen, anything more than one
        // ahead hasn't entered it. Notes are sorted, so this is a slice.
        let visible_lo = self.notes.partition_point(|n| n.time < now - APPROACH);

        // Connectors between letters of the same word
        for w in self.notes[visible_lo.saturating_sub(1)..].windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if a.time - now > APPROACH {
                break;
            }
            if a.word != b.word || a.state != NoteState::Pending || b.state != NoteState::Pending {
                continue;
            }
            let pa = self.note_pos(a, &g, now) + vec2(ox, oy);
            let pb = self.note_pos(b, &g, now) + vec2(ox, oy);
            if pa.y < g.top - 40.0 && pb.y < g.top - 40.0 {
                continue;
            }
            draw_line(pa.x, pa.y, pb.x, pb.y, 2.0, Color::new(1.0, 1.0, 1.0, 0.13));
        }

        // Gems
        let radius = (g.lane_w * 0.26).min(24.0);
        for n in &self.notes[visible_lo..] {
            if n.time - now > APPROACH {
                break;
            }
            let pos = self.note_pos(n, &g, now) + vec2(ox, oy);
            if pos.y < g.top - 40.0 || pos.y > h + 40.0 {
                continue;
            }
            match n.state {
                NoteState::Pending => {
                    // Dark-bodied gem with a lane-colored ring and letter —
                    // reads as part of the theme instead of a solid disc
                    let closeness =
                        (1.0 - ((n.time - now) / APPROACH).clamp(0.0, 1.0) as f32).powi(2);
                    let lane_c = th().lane[n.lane];
                    let mut glow = lane_c;
                    glow.a = 0.08 + 0.20 * closeness;
                    draw_circle(pos.x, pos.y, radius + 6.0 + 4.0 * closeness, glow);
                    draw_circle(pos.x, pos.y, radius, mix(th().bg, lane_c, 0.16));
                    let ring = if n.sp_phrase.is_some() { th().accent } else { lane_c };
                    draw_circle_lines(pos.x, pos.y, radius, 2.5, wa(ring, 0.75 + 0.25 * closeness));
                    if n.sp_phrase.is_some() {
                        draw_circle_lines(pos.x, pos.y, radius + 4.0, 1.5, wa(th().accent, 0.45));
                    }
                    let label = n.ch.to_ascii_uppercase().to_string();
                    let dims = msize(&label, 30);
                    dtext(
                        &label,
                        pos.x - dims.width / 2.0,
                        pos.y + dims.height / 2.0,
                        30.0,
                        mix(lane_c, WHITE, 0.25),
                    );
                }
                NoteState::Missed => {
                    draw_circle(pos.x, pos.y, radius, mix(th().bg, th().miss, 0.12));
                    draw_circle_lines(pos.x, pos.y, radius, 2.0, wa(th().miss, 0.4));
                    let label = n.ch.to_ascii_uppercase().to_string();
                    let dims = msize(&label, 30);
                    dtext(
                        &label,
                        pos.x - dims.width / 2.0,
                        pos.y + dims.height / 2.0,
                        30.0,
                        wa(th().miss, 0.45),
                    );
                }
                NoteState::Hit(_) => {}
            }
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
            let dims = msize(&f.text, f.size as u16);
            dtext(&f.text, f.pos.x - dims.width / 2.0 + ox, f.pos.y + oy, f.size, c);
        }

        // Word queue below the strike line: the current word large with live
        // per-letter results, upcoming words stacked beneath it smaller and
        // dimmer, everything easing upward as words complete
        let first_row = (self.word_anim.floor().max(0.0)) as usize;
        for wi in first_row..self.words.len() {
            let offset = wi as f32 - self.word_anim;
            if offset > 3.6 {
                break;
            }
            let y = g.hit_y + 84.0 + offset * 25.0 + oy;
            if y > h - 6.0 || y < g.hit_y + 48.0 {
                continue;
            }
            let depth = (offset.max(0.0) / 1.6).clamp(0.0, 1.0);
            let size = 44.0 - 22.0 * depth;
            // Completed words fade out as they slide above the current slot
            let row_alpha = if offset < 0.0 { (1.0 + offset).max(0.0) } else { 1.0 - 0.72 * depth };
            if row_alpha <= 0.01 {
                continue;
            }
            let word = &self.words[wi];
            let ws = self.word_starts[wi];
            let we = self.word_starts.get(wi + 1).copied().unwrap_or(self.notes.len());
            let letter_states = &self.notes[ws.min(we)..we];
            let gap = 6.0 - 2.5 * depth;
            let total_w: f32 =
                word.chars().map(|c| msize(&c.to_string(), size as u16).width + gap).sum();
            let mut x = g.left + g.width / 2.0 - total_w / 2.0 + ox;
            for (i, c) in word.chars().enumerate() {
                let mut color = match letter_states.get(i).map(|n| n.state) {
                    Some(NoteState::Hit(j)) => {
                        let mut c = j.color();
                        c.a = 0.9;
                        c
                    }
                    Some(NoteState::Missed) => wa(th().miss, 0.9),
                    _ => Color::new(1.0, 1.0, 1.0, 0.55),
                };
                color.a *= row_alpha;
                let s = c.to_string();
                dtext(&s, x, y, size, color);
                x += msize(&s, size as u16).width + gap;
            }
        }

        // Progress bar
        let resolved = self.notes.iter().filter(|n| n.state != NoteState::Pending).count();
        let frac = resolved as f32 / self.notes.len().max(1) as f32;
        draw_rectangle(g.left, 58.0, g.width, 3.0, Color::new(1.0, 1.0, 1.0, 0.12));
        draw_rectangle(g.left, 58.0, g.width * frac, 3.0, wa(th().secondary, 0.8));

        // Star power: gold wash while active, energy meter when banked
        let sp_on = self.sp_active(now);
        if sp_on {
            draw_rectangle(g.left + ox, 0.0, g.width, h, wa(th().accent, 0.05));
            draw_line(
                g.left + ox,
                g.hit_y + oy,
                g.left + g.width + ox,
                g.hit_y + oy,
                4.0,
                wa(th().accent, 0.8),
            );
        }
        if self.energy > 0.0 || sp_on {
            let bar_w = 180.0;
            let bx = 24.0;
            let by = 96.0;
            draw_rectangle(bx, by, bar_w, 8.0, Color::new(1.0, 1.0, 1.0, 0.12));
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
            draw_rectangle(bx, by, bar_w * fill, 8.0, c);
            if self.energy >= 0.5 && !sp_on {
                dtext("SPACE: star power", bx, by + 24.0, 16.0, wa(th().accent, 0.8));
            }
        }

        // HUD
        dtext(&format!("{}", self.score), 24.0, 42.0, 38.0, WHITE);
        let mult_color = if sp_on { wa(th().accent, 1.0) } else { wa(th().accent, 0.9) };
        dtext(&format!("x{}", self.multiplier(now)), 24.0, 72.0, 24.0, mult_color);
        let acc_text = format!("{:>5.1} %", self.accuracy());
        let ad = msize(&acc_text, 26);
        dtext(
            &acc_text,
            screen_width() - ad.width - 24.0,
            40.0,
            26.0,
            Color::new(1.0, 1.0, 1.0, 0.85),
        );
        let song_text = format!("{}  ·  {}", self.title, self.diff_name);
        let sd = msize(&song_text, 18);
        dtext(
            &song_text,
            screen_width() - sd.width - 24.0,
            64.0,
            18.0,
            Color::new(1.0, 1.0, 1.0, 0.4),
        );

        // Combo
        if self.combo >= 4 {
            let text = format!("{}", self.combo);
            let size = 64.0 + (self.combo.min(60) as f32) * 0.4;
            let dims = msize(&text, size as u16);
            dtext(
                &text,
                g.left + g.width / 2.0 - dims.width / 2.0 + ox,
                g.hit_y - 130.0 + oy,
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
            let dims = msize(&text, 80);
            dtext(
                &text,
                g.left + g.width / 2.0 - dims.width / 2.0,
                h * 0.4,
                80.0,
                Color::new(1.0, 1.0, 1.0, 0.5 + 0.5 * self.beat_flash),
            );
        }
    }
}

// ---------------------------------------------------------------- results

struct Results {
    song_ref: SongRef,
    title: String,
    diff_name: String,
    score: i64,
    max_combo: i64,
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
    strays: u32,
    accuracy: f64,
}

impl Results {
    fn grade(&self) -> (&'static str, Color) {
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

fn draw_centered(text: &str, y: f32, size: f32, color: Color) {
    let dims = msize(text, size as u16);
    dtext(text, screen_width() / 2.0 - dims.width / 2.0, y, size, color);
}

// ---------------------------------------------------------------- main

/// Latency calibration: tap along to a metronome, apply the median offset.
struct Calibrate {
    taps: Vec<f64>,       // signed tap offsets vs the nearest tick, seconds
    scheduled_until: f64, // timeline time up to which ticks are queued
}

enum Scene {
    Menu { sel: usize, diff_sel: usize, scroll: f32 },
    Loading { rx: Receiver<LoadMsg>, song: usize, diff: usize, title: String },
    Playing(Box<Play>),
    Results(Results),
    Calibrate(Calibrate),
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
    let engine = AudioEngine::new();
    let sounds = make_sounds(engine.sample_rate);
    let (songs, scan_errors) = chart::scan_songs(std::path::Path::new("songs"));
    let mut stem_cache: StemCache = None;
    // The most recent load failure, shown in the menu until the next attempt
    let mut status_error: Option<String> = None;
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

    loop {
        // Buffers the audio callback retired get freed here, off the
        // real-time thread
        engine.reap();
        match &mut scene {
            Scene::Menu { sel, diff_sel, scroll } => {
                let rows = songs.len();
                if rows == 0 {
                    clear_background(th().bg);
                    draw_centered("KEYBOARD WARRIOR", 130.0, 72.0, Color::new(1.0, 1.0, 1.0, 0.95));
                    draw_centered(
                        "no songs found — drop a Clone Hero .sng or song folder into songs/",
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
                    *diff_sel = 0;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Down) {
                    *sel = (*sel + 1) % rows;
                    *diff_sel = 0;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Left) && *diff_sel > 0 {
                    *diff_sel -= 1;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::Right) && *diff_sel + 1 < diff_opts.len() {
                    *diff_sel += 1;
                    engine.play(&sounds.hat, 0.4);
                }
                if is_key_pressed(KeyCode::T) {
                    let i = THEME_IDX.load(Ordering::Relaxed);
                    THEME_IDX.store((i + 1) % THEMES.len(), Ordering::Relaxed);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::M) {
                    SENTENCE_MODE.store(!sentence_mode(), Ordering::Relaxed);
                    engine.play(&sounds.kick, 0.4);
                }
                if is_key_pressed(KeyCode::C) {
                    engine.play(&sounds.kick, 0.4);
                    engine.start_timeline(1.0, None, None);
                    scene = Scene::Calibrate(Calibrate { taps: Vec::new(), scheduled_until: 0.0 });
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

                // The song list is a wheel: the selected row eases to the
                // center of the band and rows fade/shrink with distance, so a
                // large library scrolls while the bottom UI never moves.
                let dtf = get_frame_time();
                *scroll += (*sel as f32 - *scroll) * (1.0 - (-dtf * 12.0).exp());
                let hint_top = screen_height() - 130.0 - 122.0; // keyboard legend top
                let band_top = 222.0;
                let band_bot = hint_top - 26.0;
                let cy = (band_top + band_bot) / 2.0;
                let spacing = 92.0;
                for (row, song) in songs.iter().enumerate() {
                    let off = row as f32 - *scroll;
                    let y = cy + off * spacing;
                    if y < band_top - 24.0 || y > band_bot + 24.0 {
                        continue;
                    }
                    // Wheel opacity: fade with distance from the center and
                    // extinguish completely at the band edges
                    let edge = (((y - band_top) / 70.0).min((band_bot - y) / 70.0)).clamp(0.0, 1.0);
                    let a = (1.0 - off.abs() / 3.4).clamp(0.0, 1.0) * edge;
                    if a <= 0.02 {
                        continue;
                    }
                    let size = 40.0 - 6.0 * off.abs().min(2.0);
                    let selected = row == *sel;
                    let (title, subtitle) = (&song.title, &song.artist);
                    let name_color = if selected {
                        wa(th().secondary, a)
                    } else {
                        Color::new(1.0, 1.0, 1.0, 0.45 * a)
                    };
                    if selected {
                        let dims = msize(title, size as u16);
                        dtext(
                            ">",
                            screen_width() / 2.0 - dims.width / 2.0 - 40.0,
                            y,
                            size,
                            Color::new(1.0, 1.0, 1.0, (0.5 + 0.5 * pulse) * a),
                        );
                    }
                    draw_centered(title, y, size, name_color);
                    draw_centered(
                        subtitle,
                        y + 24.0,
                        18.0,
                        Color::new(1.0, 1.0, 1.0, if selected { 0.55 * a } else { 0.25 * a }),
                    );
                    if selected {
                        // Difficulty selector for this row
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
                            y + 48.0,
                            20.0,
                            wa(th().accent, 0.85 * a),
                        );
                    }
                }

                let hint_y = screen_height() - 130.0;
                draw_keyboard_legend(screen_width() / 2.0, hint_y - 122.0);
                draw_centered(
                    "type each gem on the beat  ·  miss and the lead drops out of the mix",
                    hint_y,
                    20.0,
                    Color::new(1.0, 1.0, 1.0, 0.45),
                );
                draw_centered(
                    "gold gems build star power  ·  SPACE unleashes it for 2x score",
                    hint_y + 28.0,
                    20.0,
                    wa(th().accent, 0.45),
                );
                let text_mode = if sentence_mode() { "SENTENCES" } else { "WORDS" };
                let off_ms = CALIB_MS.load(Ordering::Relaxed);
                draw_centered(
                    &format!(
                        "M — text: {}    ·    T — theme: {}    ·    C — calibrate ({off_ms:+} ms)",
                        text_mode,
                        th().name
                    ),
                    hint_y + 56.0,
                    20.0,
                    wa(th().secondary, 0.7),
                );
                draw_centered(
                    "up/down: song   ·   left/right: difficulty   ·   enter: play   ·   esc: pause",
                    hint_y + 84.0,
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
                            &format!("loading  {}", title),
                            screen_height() * 0.44,
                            30.0,
                            WHITE,
                        );
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
                if is_key_pressed(KeyCode::Escape) {
                    engine.stop_timeline();
                    scene = Scene::Menu { sel: 0, diff_sel: 0, scroll: 0.0 };
                    next_frame().await;
                    continue;
                }
                let ready = cal.taps.len() >= 4;
                if is_key_pressed(KeyCode::Enter) && ready {
                    let ms = (median(&cal.taps) * 1000.0).round() as i64;
                    CALIB_MS.store(ms, Ordering::Relaxed);
                    engine.stop_timeline();
                    engine.play(&sounds.kick, 0.5);
                    scene = Scene::Menu { sel: 0, diff_sel: 0, scroll: 0.0 };
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
                let ld = msize("late", 15);
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
        }

        next_frame().await;
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn generated_text_fits_note_counts_exactly() {
        // WORDS mode: each word length matches its phrase
        SENTENCE_MODE.store(false, std::sync::atomic::Ordering::Relaxed);
        let groups = vec![1, 2, 3, 4, 5, 6, 7, 8, 5, 3, 4, 4, 6, 2];
        let words = generate_text(&groups);
        assert_eq!(words.len(), groups.len());
        for (w, &g) in words.iter().zip(&groups) {
            assert_eq!(w.len(), g, "word {w:?} should have {g} letters");
        }
        // No adjacent repeats in a normal-sized run
        let many = generate_text(&vec![4; 40]);
        for pair in many.windows(2) {
            assert_ne!(pair[0], pair[1], "adjacent repeat: {:?}", pair);
        }

        // SENTENCES mode: total letters == total notes, whatever the grouping
        SENTENCE_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
        for total in [10usize, 57, 153, 309, 507] {
            let mut groups = vec![3usize; total / 3];
            match total % 3 {
                0 => {}
                r => groups.push(r),
            }
            let words = generate_text(&groups);
            let letters: usize = words.iter().map(|w| w.len()).sum();
            assert_eq!(letters, total);
            for w in &words {
                assert!(
                    w.chars().all(|c| c.is_ascii_lowercase() || matches!(c, ',' | '.' | '\'')),
                    "bad word {w:?}"
                );
            }
        }
        SENTENCE_MODE.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
