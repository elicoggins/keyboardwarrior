// What rides on the gems: the text modes, word pools,
// and the generators that deal a chart its words.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// What rides on the gems.
#[derive(Clone, Copy, PartialEq)]
pub enum TextMode {
    Words,      // length-matched real words, one per phrase
    WordsFixed, // words, but a chart+difficulty always deals the same ones
    Dfjk,       // four keys, four lanes — gems are amalgams of d/f/j/k
    Practice,   // random letters from a player-tuned key set
}

pub const TEXT_MODES: [(TextMode, &str); 4] = [
    (TextMode::Words, "WORDS"),
    (TextMode::WordsFixed, "WORDS (FIXED)"),
    (TextMode::Dfjk, "DFJK"),
    (TextMode::Practice, "PRACTICE"),
];
pub static TEXT_MODE_IDX: AtomicUsize = AtomicUsize::new(0);

// Typing-practice filters: which parts of the keyboard the letters come from
pub static PRAC_LEFT: AtomicBool = AtomicBool::new(true);
pub static PRAC_RIGHT: AtomicBool = AtomicBool::new(true);
pub static PRAC_TOP: AtomicBool = AtomicBool::new(true);
pub static PRAC_HOME: AtomicBool = AtomicBool::new(true);
pub static PRAC_BOTTOM: AtomicBool = AtomicBool::new(true);
pub static PRAC_PUNCT: AtomicBool = AtomicBool::new(true);

pub fn text_mode() -> TextMode {
    TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].0
}

/// Stable label for the current text mode, used to scope persisted state
/// (like high scores) per mode.
pub fn text_mode_label() -> &'static str {
    TEXT_MODES[TEXT_MODE_IDX.load(Ordering::Relaxed) % TEXT_MODES.len()].1
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

/// Deterministic seed for WORDS (FIXED): FNV-1a over the song title with the
/// difficulty mixed in, so one chart always deals one word order, but each
/// difficulty of a song gets its own.
pub fn chart_seed(title: &str, diff: usize) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in title.bytes().chain([diff as u8]) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Generate the text for a run. `groups` are the phrase sizes (note counts).
/// WORDS: one length-matched word per phrase. DFJK: amalgams of the
/// four lane keys. PRACTICE: random letters from the player-tuned key set.
pub fn generate_text(groups: &[usize]) -> Vec<String> {
    match text_mode() {
        TextMode::Words | TextMode::WordsFixed => {
            let mut decks: Vec<WordDeck> = WORDS_BY_LEN.iter().map(|p| WordDeck::new(p)).collect();
            groups
                .iter()
                .map(|&len| {
                    let idx = (len - 1).min(WORDS_BY_LEN.len() - 1);
                    decks[idx].next().to_string()
                })
                .collect()
        }
        TextMode::Dfjk => {
            groups.iter().map(|&len| random_word(len, &['d', 'f', 'j', 'k'])).collect()
        }
        TextMode::Practice => {
            let keys = practice_keys();
            groups.iter().map(|&len| random_word(len, &keys)).collect()
        }
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;
    use crate::play::gem_lane;

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

        // WORDS (FIXED): the same seed always deals the same words, and each
        // difficulty of a song seeds differently
        TEXT_MODE_IDX
            .store(TEXT_MODES.iter().position(|m| m.0 == TextMode::WordsFixed).unwrap(), Relaxed);
        macroquad::rand::srand(chart_seed("Free Bird", 2));
        let first = generate_text(&groups);
        macroquad::rand::srand(chart_seed("Free Bird", 2));
        assert_eq!(first, generate_text(&groups), "same chart seed must deal the same words");
        macroquad::rand::srand(chart_seed("Free Bird", 3));
        assert_ne!(first, generate_text(&groups), "another difficulty deals its own words");
        assert_ne!(chart_seed("Free Bird", 0), chart_seed("Free Bird", 1));
        assert_ne!(chart_seed("Free Bird", 0), chart_seed("Freebird", 0));

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
