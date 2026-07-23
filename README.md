# Keyboard Warrior

A rhythm typing game, copying off the homework of the wonderful project [Clone Hero](https://clonehero.net).

## Run

```sh
cargo run --release
```

## Practice mode

Hold **SHIFT** when pressing **ENTER** on a song to practice it Clone Hero
style: pick the section where practice starts, then the section where it ends
(the same row twice loops just that one), and the span plays on repeat with a
count-in before each pass. **LEFT / RIGHT** arrows change the game speed from
25% to 200% in 5% steps — slow motion for drilling, overclock for burn-in.
The pause screen (**ESC**) can restart the section, change which sections are
looped, or adjust the speed. Charts without section markers get automatic
8-measure parts.

## Acquiring songs

A few freely-licensed songs are bundled to play out of the box (see
[songs/README.md](songs/README.md) for licenses).

Drop a Clone Hero song into `songs/` (or point at an existing folder — see below)
and play. Both forms work as-is:

- **A `.sng` file**
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats.

### Multiple song directories

If you already have a Clone Hero directory, you can point Keyboard Warrior at it.

Added folders are stored in a small config file
(`~/Library/Application Support/keyboardwarrior/config.toml` on macOS,
`~/.config/keyboardwarrior/config.toml` on Linux) — a plain list you can also
hand-edit:

```toml
song_dirs = [
  "/Users/you/Clone Hero/Songs",
]
```

You can also set `KW_SONG_DIRS` (an OS-path-list, like `PATH`) to add folders
for a single run without saving them. The bundled `songs/` dir is always
scanned regardless.

## Tech

- **Rust + [macroquad](https://macroquad.rs)** for rendering/input
- **cpal** — output stream
- **symphonia** — ogg-vorbis / mp3 / wav / flac decoding
- **ogg + libopus** — .opus stems
- **midly** — RB-style `notes.mid`
