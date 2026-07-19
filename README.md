# Keyboard Warrior

A rhythm typing game, copying off the homework of the wonderful project [Clone Hero](https://clonehero.net).

## Run

```sh
cargo run --release
```

## Acquiring songs

A few freely-licensed songs are bundled to play out of the box (see
[songs/README.md](songs/README.md) for licenses).

Drop a Clone Hero song into `songs/` (or one of your extra folders — see below)
and play. Both forms work as-is:

- **A `.sng` file** (exactly what [Chorus Encore](https://www.enchor.us) hands
  you — no unpacking, no rebuild)
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats.

### Extra song folders

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
