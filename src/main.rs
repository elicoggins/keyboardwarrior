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
// How long a note is visible before it reaches the strike line — the board
// scroll speed. V cycles the presets in the menu; shorter time = faster.
const SPEEDS: [(&str, f64); 4] = [("SLOW", 2.6), ("NORMAL", 2.0), ("FAST", 1.5), ("TURBO", 1.1)];
static SPEED_IDX: AtomicUsize = AtomicUsize::new(1);

fn approach() -> f64 {
    SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].1
}
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
static SUSTAINS: AtomicBool = AtomicBool::new(true);

/// What rides on the gems.
#[derive(Clone, Copy, PartialEq)]
enum TextMode {
    Words,     // length-matched real words, one per phrase
    Sentences, // coherent sentences streamed letter by letter
    Dfjk,      // four keys, four lanes — gems are amalgams of d/f/j/k
    Practice,  // random letters from a player-tuned key set
}

const TEXT_MODES: [(TextMode, &str); 4] = [
    (TextMode::Words, "WORDS"),
    (TextMode::Sentences, "SENTENCES"),
    (TextMode::Dfjk, "DFJK"),
    (TextMode::Practice, "PRACTICE"),
];
static TEXT_MODE_IDX: AtomicUsize = AtomicUsize::new(0);

// Typing-practice filters: which parts of the keyboard the letters come from
static PRAC_LEFT: AtomicBool = AtomicBool::new(true);
static PRAC_RIGHT: AtomicBool = AtomicBool::new(true);
static PRAC_TOP: AtomicBool = AtomicBool::new(true);
static PRAC_HOME: AtomicBool = AtomicBool::new(true);
static PRAC_BOTTOM: AtomicBool = AtomicBool::new(true);
static PRAC_PUNCT: AtomicBool = AtomicBool::new(true);

fn th() -> &'static Theme {
    &THEMES[THEME_IDX.load(Ordering::Relaxed) % THEMES.len()]
}

fn text_mode() -> TextMode {
    TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].0
}

fn sustains_on() -> bool {
    SUSTAINS.load(Ordering::Relaxed)
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
//
// macroquad rasterizes glyphs per (character, pixel size) into one shared
// atlas, and any frame that adds a glyph re-uploads the entire atlas texture
// (and occasionally doubles it). Several text sizes here animate continuously
// — the word queue, the combo counter, the menu wheel — which would mint new
// pixel sizes almost every frame and stutter. So glyphs are only rasterized
// at SIZE_STEP-quantized sizes; font_scale closes the gap by scaling the
// cached quad, which costs nothing.
const SIZE_STEP: f32 = 4.0;

fn qsize(size: f32) -> (u16, f32) {
    // Bucket capped at 200 px: a corrupt/huge size scales a cached quad up
    // instead of rasterizing a giant glyph that would explode the atlas
    let bucket = (size / SIZE_STEP).ceil().clamp(1.0, 50.0) * SIZE_STEP;
    (bucket as u16, size / bucket)
}

fn dtext(t: &str, x: f32, y: f32, size: f32, color: Color) {
    let (font_size, font_scale) = qsize(size);
    draw_text_ex(t, x, y, TextParams { font_size, font_scale, color, ..Default::default() });
}

fn msize(t: &str, size: f32) -> TextDimensions {
    let (font_size, font_scale) = qsize(size);
    measure_text(t, None, font_size, font_scale)
}

/// Rasterize every glyph the game can draw once, at startup, so the atlas
/// never grows or re-uploads mid-song. (measure_text caches glyphs too.)
fn prewarm_glyphs() {
    let charset: String = (' '..='~').chain(['·']).collect();
    let mut bucket = SIZE_STEP;
    while bucket <= 96.0 {
        measure_text(&charset, None, bucket as u16, 1.0);
        bucket += SIZE_STEP;
    }
    // The results-screen grade is the one glyph drawn larger
    measure_text("SABCD", None, 160, 1.0);
}

// Word pools indexed by length - 1. In WORDS mode a phrase with N notes gets
// a word with N letters, so typing a word IS playing a lick. Pools are large
// and dealt from a reshuffling deck, so repeats are rare and never adjacent.
const WORDS_BY_LEN: [&[&str]; 8] = [
    // One-note phrases play every letter solo — variety beats word-ness here
    &[
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z",
    ],
    &[
        "go", "up", "on", "we", "it", "my", "do", "so", "am", "an", "as", "at", "be", "by", "he",
        "if", "in", "is", "me", "no", "of", "or", "to", "us", "ah", "aw", "ax", "ay", "bi", "bo",
        "eh", "em", "en", "ex", "hi", "ho", "id", "la", "lo", "ma", "mu", "nu", "oh", "ok", "ow",
        "ox", "oy", "pa", "pi", "re", "ta", "ti", "uh", "um", "un", "ya", "ye", "yo",
    ],
    &[
        "the", "and", "for", "you", "not", "are", "all", "new", "was", "can", "has", "but", "our",
        "one", "may", "out", "use", "any", "see", "his", "who", "web", "now", "get", "how", "its",
        "top", "had", "day", "two", "buy", "her", "add", "she", "set", "map", "way", "off", "did",
        "car", "own", "end", "him", "per", "big", "law", "art", "old", "non", "why", "low", "man",
        "job", "too", "men", "box", "gay", "air", "yes", "hot", "say", "san", "tax", "got", "let",
        "act", "red", "key", "few", "age", "pay", "war", "fax", "yet", "rss", "run", "put", "try",
        "log", "fun", "lot", "ask", "due", "pro", "ago", "via", "bad", "far", "oil", "bit", "bay",
        "bar", "dog", "gas", "six", "pre", "zip", "bid", "inn", "los", "win", "bed", "sea", "cut",
        "tel", "kit", "boy", "son", "mac", "bin", "van", "ads", "pop", "hit", "eye", "fee", "las",
        "aid", "fat", "saw", "tom", "led", "fan", "ten", "cat", "die", "pet", "guy", "dev", "cup",
        "lee", "bob", "fit", "met", "ice", "sec", "bus", "bag", "ibm",
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
        "chat", "loss", "bird", "bear", "bell", "barn", "calm", "cave", "clay", "coal", "cool",
        "cozy", "crab", "crow", "dawn", "deer", "dish", "dove", "drum", "dusk", "fern", "flag",
        "foam", "frog", "gate", "gaze", "glow", "goat", "gulf", "hawk", "hill", "hive", "hush",
        "kite", "lamb", "leaf", "lime", "lush", "mint", "mist", "moon", "moss", "myth", "nest",
        "palm", "pine", "pond", "rain", "reef", "sail", "sand", "seed", "silk", "snow", "song",
        "surf", "swan", "tide", "twig", "vine", "wave", "wind", "wolf", "wool", "yarn", "zoom",
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
        "amber", "apple", "berry", "birch", "bloom", "brave", "bread", "brick", "brook", "candy",
        "cedar", "charm", "chess", "chill", "cider", "cloud", "coral", "crane", "cream", "crisp",
        "crown", "dance", "dream", "drift", "eagle", "ember", "fable", "fairy", "feast", "fence",
        "fever", "flame", "flock", "flora", "flute", "frost", "glaze", "gleam", "globe", "grace",
        "grape", "grove", "happy", "hazel", "honey", "horse", "juice", "koala", "lemon", "lilac",
        "lunar", "mango", "maple", "melon", "merry", "mocha", "moose", "noble", "ocean", "olive",
        "onion", "opera", "otter", "panda", "peach", "pearl", "pedal", "penny", "piano", "pilot",
        "pixel", "plaza", "polar", "prism", "quilt", "raven", "ridge", "roast", "robin", "royal",
        "salsa", "shine", "shore", "smile", "spark", "spice", "storm", "sugar", "sunny", "swirl",
        "tiger", "toast", "torch", "trail", "tribe", "tulip", "vivid", "wagon", "whale", "wheat",
        "zebra",
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
        "bakery", "ballet", "bamboo", "banana", "basket", "beacon", "bottle", "branch", "breeze",
        "bridge", "bright", "bronze", "bubble", "bucket", "butter", "candle", "canyon", "carpet",
        "castle", "celery", "cereal", "cheese", "cherry", "circus", "clever", "closet", "cobweb",
        "coffee", "copper", "cosmic", "cotton", "coyote", "cradle", "crayon", "dazzle", "decade",
        "desert", "dinner", "doctor", "donkey", "dragon", "drawer", "effort", "eleven", "empire",
        "escape", "exotic", "fabric", "falcon", "fiddle", "finger", "flavor", "forest", "fossil",
        "frozen", "galaxy", "garage", "gentle", "ginger", "golden", "guitar", "hammer", "harbor",
        "helmet", "hidden", "hollow", "honest", "hunter", "indigo", "insect", "jacket", "jaguar",
        "jungle", "kettle", "kitten", "ladder", "lagoon", "laptop", "legend", "lively", "lizard",
        "locker", "lumber", "magnet", "marble", "meadow", "melody", "mellow", "mirror", "mitten",
        "modest", "monkey", "mosaic", "mother", "muffin", "museum", "mystic", "native", "nectar",
        "noodle", "nugget", "oyster", "paddle", "palace", "parade", "pastel", "pebble", "pencil",
        "pepper", "picnic", "pigeon", "pillow", "pirate", "planet", "pocket", "polish", "pollen",
        "poster", "potato", "pretty", "purple", "puzzle", "rabbit", "raisin", "ripple", "rocket",
        "rubber", "saddle", "salmon", "shadow", "shrimp", "signal", "sonnet", "spider", "spiral",
        "splash", "spring", "sprout", "squash", "stable", "stereo", "stitch", "stormy", "stream",
        "string", "sturdy", "subtle", "sunset", "temple", "tender", "thrive", "timber", "tomato",
        "trophy", "turtle", "tuxedo", "twelve", "valley", "velvet", "violet", "violin", "voyage",
        "waffle", "walnut", "walrus", "wander", "wallet", "warmth", "willow", "winter", "wisdom",
        "wizard", "wonder", "yogurt", "zigzag",
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
        "sellers", "tuesday", "machine", "morning", "evening", "weekend", "journey", "freedom",
        "courage", "diamond", "emerald", "thunder", "rainbow", "sunrise", "horizon", "volcano",
        "tornado", "whisper", "mystery", "magical", "crystal", "feather", "blanket", "caramel",
        "capture", "captain", "cartoon", "ceiling", "century", "channel", "charity", "chicken",
        "chimney", "clarity", "classic", "climate", "clothes", "coastal", "comfort", "compass",
        "concert", "cottage", "cricket", "crimson", "cupcake", "curious", "custard", "dolphin",
        "drawing", "dreamer", "eclipse", "economy", "elegant", "embrace", "emotion", "explore",
        "factory", "fantasy", "fiction", "fitness", "forever", "fortune", "forward", "genuine",
        "giraffe", "glacier", "glitter", "granite", "gravity", "grocery", "habitat", "hallway",
        "hammock", "harmony", "harvest", "highway", "husband", "iceberg", "imagine", "inspire",
        "instant", "jasmine", "justice", "kitchen", "lantern", "laundry", "leather", "lettuce",
        "liberty", "lobster", "mansion", "mineral", "miracle", "monarch", "monsoon", "monster",
        "musical", "mustang", "nowhere", "oatmeal", "obvious", "octopus", "orchard", "organic",
        "outdoor", "outside", "pancake", "panther", "partner", "passion", "pathway", "patient",
        "peacock", "penguin", "perfect", "phantom", "pioneer", "playful", "popcorn", "prairie",
        "pretzel", "promise", "pumpkin", "pyramid", "quietly", "radiant", "reality", "reflect",
        "respect", "rooster", "sailing", "sausage", "seagull", "seaside", "shelter", "silence",
        "sincere", "skyline", "sparrow", "spinach", "stadium", "station", "stellar", "strange",
        "stretch", "sunbeam", "supreme", "sweater", "teacher", "theater", "tonight", "tractor",
        "triumph", "trumpet", "unicorn", "uniform", "vampire", "vanilla", "victory", "village",
        "vintage", "visitor", "wealthy", "whistle", "wildcat", "workout",
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
        "featured", "friendly", "schedule", "everyone", "approach", "mountain", "sunshine",
        "thinking", "daughter", "favorite", "elephant", "umbrella", "sandwich", "birthday",
        "notebook", "keyboard", "painting", "distance", "medicine", "straight", "strength",
        "shoulder", "treasure", "campaign", "festival", "hospital", "universe", "creative",
        "pleasure", "surprise", "champion", "midnight", "daylight", "backyard", "baseball",
        "football", "swimming", "vacation", "airplane", "dinosaur", "squirrel", "kangaroo",
        "laughter", "graceful", "powerful", "peaceful", "grateful", "handsome", "generous",
        "absolute", "accurate", "alphabet", "ancestor", "appetite", "attitude", "audience",
        "backpack", "bathroom", "blizzard", "boundary", "broccoli", "carnival", "ceremony",
        "chemical", "chestnut", "cinnamon", "clarinet", "colorful", "composer", "concrete",
        "confetti", "corridor", "creature", "crescent", "critical", "crossing", "cucumber",
        "currency", "darkness", "deadline", "delicate", "describe", "dialogue", "discover",
        "doorbell", "downtown", "dramatic", "dreaming", "driveway", "electric", "elegance",
        "emphasis", "engineer", "envelope", "estimate", "eternity", "evidence", "exercise",
        "explorer", "fabulous", "familiar", "firewood", "flamingo", "fountain", "frontier",
        "geometry", "goldfish", "graduate", "handmade", "hardware", "headline", "heavenly",
        "hedgehog", "humorous", "identity", "infinite", "innocent", "instance", "interior",
        "jealousy", "junction", "lakeside", "landmark", "lavender", "lemonade", "likewise",
        "listener", "majestic", "marathon", "meantime", "memorial", "merchant", "molecule",
        "momentum", "monument", "mosquito", "movement", "mushroom", "mystical", "neighbor",
        "nonsense", "northern", "observer", "obstacle", "occasion", "opponent", "opposite",
        "ordinary", "ornament", "outdoors", "overcome", "overlook", "paradise", "parallel",
        "particle", "passport", "patience", "peculiar", "physical", "platform", "positive",
        "precious", "presence", "princess", "priority", "quantity", "reaction", "reindeer",
        "relative", "romantic", "sapphire", "scenario", "scissors", "seashell", "seasonal",
        "sentence", "separate", "serenade", "shepherd", "sidewalk", "skeleton", "snowfall",
        "southern", "specimen", "spectrum", "stairway", "stranger", "strategy", "struggle",
        "suburban", "sunlight", "symphony", "teaspoon", "tendency", "textbook", "thousand",
        "timeless", "tomorrow", "towering", "traveler", "treasury", "triangle", "tropical",
        "twilight", "ultimate", "upstairs", "valuable", "vertical", "vineyard", "westward",
        "wildlife", "windmill", "wondrous", "workshop", "yearbook",
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

/// The keys typing practice may draw from, honoring the hand, row, and
/// punctuation filters. Punctuation keys count as right-hand. Filtering
/// everything away falls back to the home index keys.
fn practice_keys() -> Vec<char> {
    let rows: [(&str, &AtomicBool); 3] =
        [("qwertyuiop", &PRAC_TOP), ("asdfghjkl'", &PRAC_HOME), ("zxcvbnm,.", &PRAC_BOTTOM)];
    let mut keys = Vec::new();
    for (row, row_on) in rows {
        if !row_on.load(Ordering::Relaxed) {
            continue;
        }
        for c in row.chars() {
            let punct = !c.is_ascii_alphabetic();
            let left = "qwertasdfgzxcvb".contains(c);
            let hand = if left { &PRAC_LEFT } else { &PRAC_RIGHT };
            if !hand.load(Ordering::Relaxed) || (punct && !PRAC_PUNCT.load(Ordering::Relaxed)) {
                continue;
            }
            keys.push(c);
        }
    }
    if keys.is_empty() {
        keys = vec!['f', 'j'];
    }
    keys
}

/// A pseudo-word: `len` random keys from `set`, avoiding immediate doubles
/// (one re-roll — rare doubles read as intentional jacks, runs don't).
fn random_word(len: usize, set: &[char]) -> String {
    let pick = || set[macroquad::rand::gen_range(0usize, set.len()).min(set.len() - 1)];
    let mut out = String::with_capacity(len);
    let mut prev = '\0';
    for _ in 0..len {
        let mut c = pick();
        if c == prev {
            c = pick();
        }
        out.push(c);
        prev = c;
    }
    out
}

/// Generate the text for a run. `groups` are the phrase sizes (note counts).
/// WORDS: one length-matched word per phrase. SENTENCES: coherent sentences
/// streamed across the same total number of notes. DFJK: amalgams of the
/// four lane keys. PRACTICE: random letters from the player-tuned key set.
fn generate_text(groups: &[usize]) -> Vec<String> {
    match text_mode() {
        TextMode::Words => {
            let mut decks: Vec<WordDeck> = WORDS_BY_LEN.iter().map(|p| WordDeck::new(p)).collect();
            return groups
                .iter()
                .map(|&len| {
                    let idx = (len - 1).min(WORDS_BY_LEN.len() - 1);
                    decks[idx].next().to_string()
                })
                .collect();
        }
        TextMode::Dfjk => {
            return groups.iter().map(|&len| random_word(len, &['d', 'f', 'j', 'k'])).collect();
        }
        TextMode::Practice => {
            let keys = practice_keys();
            return groups.iter().map(|&len| random_word(len, &keys)).collect();
        }
        TextMode::Sentences => {}
    }

    // Sentences: deal whole sentences until the letter budget is spent,
    // topping off the tail with an exact-length word so every note has a letter
    let mut decks: Vec<WordDeck> = WORDS_BY_LEN.iter().map(|p| WordDeck::new(p)).collect();
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

fn lane_of(c: char) -> usize {
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
fn gem_lane(c: char) -> usize {
    if c == 'd' && text_mode() == TextMode::Dfjk {
        return 0;
    }
    lane_of(c)
}

/// Characters that can appear on gems: letters plus unshifted punctuation.
fn is_typeable(c: char) -> bool {
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
    holds: Vec<Hold>, // sustains currently being held
    whammying: bool,  // SHIFT is pressing the whammy bar on an active sustain
    whammy_vis: f32,  // eased bar position, drives the tail's bow on screen
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

/// Highway y for a timeline second: the strike line at `now`, the top of the
/// highway one approach-time later.
fn time_to_y(t: f64, g: &Geom, now: f64) -> f32 {
    g.hit_y - (((t - now) / approach()) as f32) * (g.hit_y - g.top)
}

/// The strike line, drawn as five segments with gaps at the lane centers so
/// it never cuts through the target rings or a gem crossing it.
fn draw_strike_line(g: &Geom, ox: f32, oy: f32, thickness: f32, color: Color) {
    let gap = 30.0;
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
    fn new_chart(
        song_idx: usize,
        diff: usize,
        chart: &SongChart,
        engine: &AudioEngine,
        snd: &Sounds,
        backing: Buf,
        lead: Option<Buf>,
    ) -> Self {
        let times: Vec<(f64, f64)> = chart.diffs[diff].iter().map(|n| (n.time, n.len)).collect();

        // Group notes into phrases at musical rests (or when a word maxes out)
        let mut groups: Vec<Vec<(f64, f64)>> = Vec::new();
        for &(t, len) in &times {
            let new_group = match groups.last().and_then(|g| g.last()) {
                Some(&(prev, _)) => t - prev > 0.85 || groups.last().unwrap().len() >= 8,
                None => true,
            };
            if new_group {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push((t, len));
        }
        // Fold lonely single-note groups into the previous word when close
        let mut merged: Vec<Vec<(f64, f64)>> = Vec::new();
        for g in groups {
            match merged.last_mut() {
                Some(prev)
                    if g.len() == 1 && prev.len() < 8 && g[0].0 - prev.last().unwrap().0 < 1.6 =>
                {
                    prev.extend(g);
                }
                _ => merged.push(g),
            }
        }

        let group_lens: Vec<usize> = merged.iter().map(|g| g.len()).collect();
        let mut flat_times: Vec<(f64, f64)> = merged.concat();
        // Sustains: only tails long enough to be worth holding, clipped so
        // they never overlap the next note's press; drop them entirely when
        // the option is off
        for i in 0..flat_times.len() {
            let next_t = flat_times.get(i + 1).map(|n| n.0);
            let (t, mut len) = flat_times[i];
            if !sustains_on() {
                len = 0.0;
            }
            if let Some(nt) = next_t {
                len = len.min(nt - t - 0.12);
            }
            if len < 0.3 {
                len = 0.0;
            }
            flat_times[i].1 = len;
        }
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
            holds: Vec::new(),
            whammying: false,
            whammy_vis: 0.0,
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
        vec2(g.left + g.lane_w * (note.lane as f32 + 0.5), time_to_y(note.time, g, now))
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

        // Sustain holds: bonus score drips in while the key stays down.
        // Lifting early just stops the bonus — no combo break, like GH.
        // SHIFT is the whammy bar: holding it keeps the lead bent down and
        // fattened, releasing returns it to normal. While pressed on a
        // sustain it also doubles the drip and trickles star power.
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
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
        let ap = approach();

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
        draw_strike_line(&g, ox, oy, 4.0, Color::new(1.0, 1.0, 1.0, flash));
        for lane in 0..4 {
            let x = g.left + g.lane_w * (lane as f32 + 0.5) + ox;
            let mut c = th().lane[lane];
            c.a = 0.30 + 0.12 * self.beat_flash;
            draw_circle_lines(x, g.hit_y + oy, 24.0, 2.0, c);
        }

        // Only notes near the screen need drawing: anything more than one
        // approach-window old is far below it. Ahead, the window is stretched
        // past the highway top so gems spawn above the window edge and drift
        // in instead of popping in at the top. Notes are sorted: a slice.
        let spawn_ap = ap * ((g.hit_y + 60.0) / (g.hit_y - g.top)) as f64;
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
            if pa.y < -60.0 && pb.y < -60.0 {
                continue;
            }
            draw_line(pa.x, pa.y, pb.x, pb.y, 2.0, Color::new(1.0, 1.0, 1.0, 0.13));
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
            if pos.y < -60.0 || pos.y > h + 40.0 {
                continue;
            }
            let y_end = (time_to_y(n.time + n.sustain, &g, now) + oy).max(-20.0);
            draw_line(pos.x, pos.y, pos.x, y_end, 5.0, wa(th().lane[n.lane], 0.22));
        }

        // Gems
        let radius = (g.lane_w * 0.26).min(24.0);
        for n in &self.notes[visible_lo..] {
            if n.time - now > spawn_ap {
                break;
            }
            let pos = self.note_pos(n, &g, now) + vec2(ox, oy);
            if pos.y < -60.0 || pos.y > h + 40.0 {
                continue;
            }
            match n.state {
                NoteState::Pending => {
                    // Dark-bodied gem with a lane-colored ring and letter —
                    // reads as part of the theme instead of a solid disc
                    let closeness = (1.0 - ((n.time - now) / ap).clamp(0.0, 1.0) as f32).powi(2);
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
                    let dims = msize(&label, 30.0);
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
                    let dims = msize(&label, 30.0);
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

        // Active holds: the remaining tail drains into a glowing anchor on
        // the strike line while the key stays down. Pressing the whammy bar
        // fattens the tail and sends a wave rippling down it — pulsing in
        // time with the audio's pitch pump — releasing lets it slim and
        // straighten again, GH-style.
        for hd in &self.holds {
            let n = &self.notes[hd.note];
            let x = g.left + g.lane_w * (n.lane as f32 + 0.5) + ox;
            let y_end = (time_to_y(n.time + n.sustain, &g, now) + oy).max(-20.0);
            let c = th().lane[n.lane];
            let vis = self.whammy_vis;
            // Same pump rate as the audio LFO, so width and pitch breathe as one
            let pump_ph = now * std::f64::consts::TAU * audio::WH_PUMP_HZ;
            let pump = (0.5 - 0.5 * pump_ph.cos()) as f32;
            let thick = 7.0 + (6.0 + 2.5 * pump) * vis;
            if vis > 0.02 {
                let anchor = g.hit_y + oy;
                let wave_t = pump_ph as f32;
                let mut prev = vec2(x, anchor);
                let mut yy = anchor - 9.0;
                loop {
                    let seg_y = yy.max(y_end);
                    let d = anchor - seg_y; // distance up the tail, px
                                            // Traveling wave, pinned at the anchor so the base stays
                                            // planted on the strike line
                    let amp = 6.5 * vis * (d / 60.0).min(1.0);
                    let p = vec2(x + (d * 0.055 + wave_t).sin() * amp, seg_y);
                    // Soft halo under the core line doubles the tail's body
                    draw_line(prev.x, prev.y, p.x, p.y, thick + 8.0, wa(c, 0.22 * vis));
                    draw_line(prev.x, prev.y, p.x, p.y, thick, wa(c, 0.78));
                    if seg_y <= y_end {
                        break;
                    }
                    prev = p;
                    yy -= 9.0;
                }
            } else {
                draw_line(x, g.hit_y + oy, x, y_end, thick, wa(c, 0.75));
            }
            let pump_r = (3.0 + 2.0 * pump) * vis;
            draw_circle(x, g.hit_y + oy, 12.0 + pump_r, wa(c, 0.9));
            draw_circle_lines(x, g.hit_y + oy, 19.0 + 3.0 * self.beat_flash, 2.0, wa(c, 0.6));
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
                    draw_line(x, y + 7.0, x + w, y + 7.0, 2.0, wa(th().accent, 0.7 * row_alpha));
                }
                x += w + gap;
            }
        }

        // Side-gutter HUD: the score column lives in the left gutter, the
        // song column in the right — nothing overlays the highway. Text
        // shrinks to fit the gutter so long titles never spill onto it.
        let lcx = g.left / 2.0;
        let rcx = g.left + g.width + (screen_width() - g.left - g.width) / 2.0;
        let col_w = (g.left - 28.0).max(60.0);
        let sp_on = self.sp_active(now);

        draw_fit("SCORE", lcx, 106.0, 15.0, col_w, Color::new(1.0, 1.0, 1.0, 0.35));
        draw_fit(&format!("{}", self.score), lcx, 146.0, 42.0, col_w, WHITE);
        let mult_color = if sp_on { wa(th().accent, 1.0) } else { wa(th().accent, 0.9) };
        draw_fit(&format!("x{}", self.multiplier(now)), lcx, 180.0, 24.0, col_w, mult_color);

        // Star power: gold wash while active, energy meter when banked
        if sp_on {
            draw_rectangle(g.left + ox, 0.0, g.width, h, wa(th().accent, 0.05));
            draw_strike_line(&g, ox, oy, 4.0, wa(th().accent, 0.8));
        }
        if self.energy > 0.0 || sp_on {
            let bar_w = col_w.min(170.0);
            let bx = lcx - bar_w / 2.0;
            let by = 208.0;
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
                draw_fit("SPACE: star power", lcx, by + 26.0, 16.0, col_w, wa(th().accent, 0.8));
            }
        }

        draw_fit(&self.title, rcx, 106.0, 22.0, col_w, Color::new(1.0, 1.0, 1.0, 0.85));
        draw_fit(&self.diff_name, rcx, 130.0, 16.0, col_w, Color::new(1.0, 1.0, 1.0, 0.45));
        let acc_text = format!("{:.1} %", self.accuracy());
        draw_fit(&acc_text, rcx, 174.0, 30.0, col_w, Color::new(1.0, 1.0, 1.0, 0.85));

        // Song completion, down in the right gutter instead of across the top
        let resolved = self.notes.iter().filter(|n| n.state != NoteState::Pending).count();
        let frac = resolved as f32 / self.notes.len().max(1) as f32;
        let pw = col_w.min(170.0);
        draw_rectangle(rcx - pw / 2.0, 204.0, pw, 4.0, Color::new(1.0, 1.0, 1.0, 0.12));
        draw_rectangle(rcx - pw / 2.0, 204.0, pw * frac, 4.0, wa(th().secondary, 0.8));
        let pct = format!("{:.0}%", frac * 100.0);
        draw_fit(&pct, rcx, 228.0, 15.0, col_w, Color::new(1.0, 1.0, 1.0, 0.4));

        // Combo
        if self.combo >= 4 {
            let text = format!("{}", self.combo);
            let size = 64.0 + (self.combo.min(60) as f32) * 0.4;
            let dims = msize(&text, size);
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
            let dims = msize(&text, 80.0);
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
    let dims = msize(text, size);
    dtext(text, screen_width() / 2.0 - dims.width / 2.0, y, size, color);
}

/// Text centered on a column, shrunk to fit its width — the side-gutter HUD
/// uses this so nothing spills onto the highway.
fn draw_fit(text: &str, cx: f32, y: f32, size: f32, max_w: f32, color: Color) {
    let mut s = size;
    let d = msize(text, s);
    if d.width > max_w {
        s *= max_w / d.width;
    }
    let d = msize(text, s);
    dtext(text, cx - d.width / 2.0, y, s, color);
}

// ---------------------------------------------------------------- diagnostics

const FRAME_LOG_LEN: usize = 240;

/// F1 overlay: recent frame times as 1px bars against a 60 fps reference
/// line, with the worst frame in the window called out. Spikes paint red.
fn draw_frame_graph(log: &std::collections::VecDeque<f32>) {
    let (w, h) = (FRAME_LOG_LEN as f32, 64.0);
    let x0 = 14.0;
    let y1 = screen_height() - 14.0;
    let scale = h / 0.034; // graph top ≈ 34 ms, two 60 Hz frames
    draw_rectangle(x0 - 6.0, y1 - h - 6.0, w + 12.0, h + 12.0, Color::new(0.0, 0.0, 0.0, 0.55));
    for (i, &dt) in log.iter().enumerate() {
        let bh = (dt * scale).clamp(1.0, h);
        let c = if dt > 1.0 / 45.0 { th().miss } else { th().secondary };
        draw_rectangle(x0 + i as f32, y1 - bh, 1.0, bh, wa(c, 0.9));
    }
    let y60 = y1 - scale / 60.0;
    draw_line(x0, y60, x0 + w, y60, 1.0, Color::new(1.0, 1.0, 1.0, 0.4));
    let worst = log.iter().copied().fold(0.0f32, f32::max);
    dtext(
        &format!("{} fps   worst {:.1} ms", get_fps(), worst * 1000.0),
        x0,
        y1 - h - 14.0,
        16.0,
        Color::new(1.0, 1.0, 1.0, 0.8),
    );
}

// ---------------------------------------------------------------- main

/// Latency calibration: tap along to a metronome, apply the median offset.
struct Calibrate {
    taps: Vec<f64>,       // signed tap offsets vs the nearest tick, seconds
    scheduled_until: f64, // timeline time up to which ticks are queued
    menu_sel: usize,      // menu selection to restore on the way back out
}

// ---------------------------------------------------------------- settings

/// One adjustable row on the settings screen.
#[derive(Clone, Copy, PartialEq)]
enum SettingRow {
    TextMode,
    PracLeft,
    PracRight,
    PracTop,
    PracHome,
    PracBottom,
    PracPunct,
    Theme,
    Sustains,
    Speed,
    Volume,
    Calibrate,
}

/// The rows currently on screen: the practice key filters only appear while
/// the text mode is PRACTICE, indented under it.
fn settings_rows() -> Vec<SettingRow> {
    let mut rows = vec![SettingRow::TextMode];
    if text_mode() == TextMode::Practice {
        rows.extend([
            SettingRow::PracLeft,
            SettingRow::PracRight,
            SettingRow::PracTop,
            SettingRow::PracHome,
            SettingRow::PracBottom,
            SettingRow::PracPunct,
        ]);
    }
    rows.extend([
        SettingRow::Theme,
        SettingRow::Sustains,
        SettingRow::Speed,
        SettingRow::Volume,
        SettingRow::Calibrate,
    ]);
    rows
}

fn cycle(idx: &AtomicUsize, n: usize, dir: i32) {
    let i = idx.load(Ordering::Relaxed) as i32 + dir;
    idx.store(i.rem_euclid(n as i32) as usize, Ordering::Relaxed);
}

fn flip(b: &AtomicBool) {
    b.store(!b.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn on_off(b: &AtomicBool) -> &'static str {
    if b.load(Ordering::Relaxed) {
        "ON"
    } else {
        "OFF"
    }
}

impl SettingRow {
    fn label(self) -> &'static str {
        match self {
            SettingRow::TextMode => "text mode",
            SettingRow::PracLeft => "left hand",
            SettingRow::PracRight => "right hand",
            SettingRow::PracTop => "top row",
            SettingRow::PracHome => "home row",
            SettingRow::PracBottom => "bottom row",
            SettingRow::PracPunct => "punctuation",
            SettingRow::Theme => "theme",
            SettingRow::Sustains => "sustains",
            SettingRow::Speed => "speed",
            SettingRow::Volume => "volume",
            SettingRow::Calibrate => "calibrate",
        }
    }

    fn indented(self) -> bool {
        matches!(
            self,
            SettingRow::PracLeft
                | SettingRow::PracRight
                | SettingRow::PracTop
                | SettingRow::PracHome
                | SettingRow::PracBottom
                | SettingRow::PracPunct
        )
    }

    fn value(self, engine: &AudioEngine) -> String {
        match self {
            SettingRow::TextMode => {
                TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].1.to_string()
            }
            SettingRow::PracLeft => on_off(&PRAC_LEFT).into(),
            SettingRow::PracRight => on_off(&PRAC_RIGHT).into(),
            SettingRow::PracTop => on_off(&PRAC_TOP).into(),
            SettingRow::PracHome => on_off(&PRAC_HOME).into(),
            SettingRow::PracBottom => on_off(&PRAC_BOTTOM).into(),
            SettingRow::PracPunct => on_off(&PRAC_PUNCT).into(),
            SettingRow::Theme => th().name.to_string(),
            SettingRow::Sustains => on_off(&SUSTAINS).into(),
            SettingRow::Speed => {
                SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].0.to_string()
            }
            SettingRow::Volume => format!("{:.0}%", engine.master() * 100.0),
            SettingRow::Calibrate => format!("{:+} ms", CALIB_MS.load(Ordering::Relaxed)),
        }
    }

    fn adjust(self, dir: i32, engine: &AudioEngine) {
        match self {
            SettingRow::TextMode => cycle(&TEXT_MODE_IDX, TEXT_MODES.len(), dir),
            SettingRow::PracLeft => flip(&PRAC_LEFT),
            SettingRow::PracRight => flip(&PRAC_RIGHT),
            SettingRow::PracTop => flip(&PRAC_TOP),
            SettingRow::PracHome => flip(&PRAC_HOME),
            SettingRow::PracBottom => flip(&PRAC_BOTTOM),
            SettingRow::PracPunct => flip(&PRAC_PUNCT),
            SettingRow::Theme => cycle(&THEME_IDX, THEMES.len(), dir),
            SettingRow::Sustains => flip(&SUSTAINS),
            SettingRow::Speed => cycle(&SPEED_IDX, SPEEDS.len(), dir),
            SettingRow::Volume => {
                engine.set_master(((engine.master() + 0.05 * dir as f32) * 20.0).round() / 20.0);
            }
            SettingRow::Calibrate => {} // ENTER opens the metronome instead
        }
    }

    fn desc(self) -> &'static str {
        match self {
            SettingRow::TextMode => match text_mode() {
                TextMode::Words => "phrases become real words sized to the beat",
                TextMode::Sentences => "coherent sentences streamed letter by letter",
                TextMode::Dfjk => "four keys, four lanes - any key in a lane's zone counts",
                TextMode::Practice => "random letters - tune which keys appear below",
            },
            SettingRow::PracLeft => "letters typed by the left hand",
            SettingRow::PracRight => "letters typed by the right hand",
            SettingRow::PracTop => "the qwerty row",
            SettingRow::PracHome => "the asdf row",
            SettingRow::PracBottom => "the zxcv row",
            SettingRow::PracPunct => "comma, period, apostrophe - shift is never needed",
            SettingRow::Theme => "lane and accent colors",
            SettingRow::Sustains => "hold long notes for bonus score",
            SettingRow::Speed => "how long notes stay on the highway",
            SettingRow::Volume => "master volume - also -/+ from anywhere",
            SettingRow::Calibrate => "ENTER: tap along to measure your keyboard latency",
        }
    }
}

enum Scene {
    Menu { sel: usize, diff_sel: usize, scroll: f32 },
    Settings { sel: usize, menu_sel: usize },
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
    prewarm_glyphs();
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

    // Frame-time overlay (F1), for chasing stutter by eye
    let mut show_frame_graph = false;
    let mut frame_log: std::collections::VecDeque<f32> =
        std::collections::VecDeque::with_capacity(FRAME_LOG_LEN);
    // Seconds left on the master-volume overlay after a -/+ press
    let mut vol_flash = 0.0f32;

    loop {
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
                if is_key_pressed(KeyCode::S) {
                    engine.play(&sounds.kick, 0.4);
                    scene = Scene::Settings { sel: 0, menu_sel: *sel };
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
                    let name_color = if selected {
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
                    "gold gems build star power  ·  SPACE unleashes it  ·  SHIFT whammies sustains",
                    hint_y + 28.0,
                    20.0,
                    wa(th().accent, 0.45),
                );
                // Read-only snapshot of the current settings; S opens them
                let mode_name = TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % 4].1;
                let sus = if sustains_on() { "ON" } else { "OFF" };
                let off_ms = CALIB_MS.load(Ordering::Relaxed);
                let speed = SPEEDS[SPEED_IDX.load(Ordering::Relaxed) % SPEEDS.len()].0;
                draw_fit(
                    &format!(
                        "{}   ·   {}   ·   sustains {}   ·   {}   ·   {off_ms:+} ms   ·   vol {:.0}%",
                        mode_name,
                        th().name,
                        sus,
                        speed,
                        engine.master() * 100.0
                    ),
                    screen_width() / 2.0,
                    hint_y + 56.0,
                    18.0,
                    screen_width() - 32.0,
                    wa(th().secondary, 0.55),
                );
                draw_centered(
                    "up/down: song   ·   left/right: difficulty   ·   enter: play   ·   S: settings",
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
                // Both exits land back on the settings screen's calibrate row
                let back = Scene::Settings {
                    sel: settings_rows()
                        .iter()
                        .position(|r| *r == SettingRow::Calibrate)
                        .unwrap_or(0),
                    menu_sel: cal.menu_sel,
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

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn word_pools_are_clean() {
        for (i, pool) in WORDS_BY_LEN.iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for w in *pool {
                assert_eq!(w.len(), i + 1, "{w:?} doesn't belong in the {}-letter pool", i + 1);
                assert!(w.chars().all(|c| c.is_ascii_lowercase()), "bad characters in {w:?}");
                assert!(seen.insert(*w), "duplicate {w:?} in the {}-letter pool", i + 1);
            }
        }
    }

    // All text-mode assertions live in this one test: the mode and practice
    // filters are process-wide statics, and parallel test threads would race
    #[test]
    fn generated_text_fits_note_counts_exactly() {
        use std::sync::atomic::Ordering::Relaxed;
        // WORDS mode: each word length matches its phrase
        TEXT_MODE_IDX.store(0, Relaxed);
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
        TEXT_MODE_IDX.store(1, Relaxed);
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

        // DFJK mode: amalgams of the four lane keys, and d pins to lane 0
        TEXT_MODE_IDX.store(2, Relaxed);
        let groups = vec![1, 4, 8, 3, 6];
        let words = generate_text(&groups);
        for (w, &g) in words.iter().zip(&groups) {
            assert_eq!(w.len(), g);
            assert!(w.chars().all(|c| "dfjk".contains(c)), "bad dfjk word {w:?}");
        }
        assert_eq!(gem_lane('d'), 0);
        assert_eq!(gem_lane('f'), 1);
        assert_eq!(gem_lane('j'), 2);
        assert_eq!(gem_lane('k'), 3);
        assert_eq!(gem_lane('q'), 0, "q aims at the D lane");
        assert_eq!(gem_lane(','), 3, "punctuation aims at the K lane");

        // PRACTICE mode: filters shape the key set, lengths still match
        TEXT_MODE_IDX.store(3, Relaxed);
        PRAC_RIGHT.store(false, Relaxed);
        PRAC_PUNCT.store(false, Relaxed);
        let words = generate_text(&[6; 20]);
        for w in &words {
            assert_eq!(w.len(), 6);
            assert!(
                w.chars().all(|c| "qwertasdfgzxcvb".contains(c)),
                "right hand was off, got {w:?}"
            );
        }
        // Everything off still yields a playable set (home index fallback)
        PRAC_LEFT.store(false, Relaxed);
        assert_eq!(practice_keys(), vec!['f', 'j']);

        // Restore defaults for any test that runs after in this process
        PRAC_LEFT.store(true, Relaxed);
        PRAC_RIGHT.store(true, Relaxed);
        PRAC_PUNCT.store(true, Relaxed);
        TEXT_MODE_IDX.store(0, Relaxed);
    }
}
