# Keyboard Warrior

A rhythm typing game, copying off the homework of the wonderful project [Clone Hero](https://clonehero.net).

## Run

```sh
cargo run --release
```

## Adding songs — zero conversion

The song library is read at launch — nothing is baked into the binary. A few
freely-licensed songs are bundled to play out of the box (see
[songs/README.md](songs/README.md) for licenses); they always stay in the
library, and anything you add sorts above them.

Drop a Clone Hero song into `songs/` (or one of your extra folders — see below)
and play. Both forms work as-is:

- **A `.sng` file** (exactly what [Chorus Encore](https://www.enchor.us) hands
  you — no unpacking, no rebuild)
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats.

### From inside the game

You don't have to touch the filesystem at all. From the song menu:

- **`G` — get songs:** search [Chorus Encore](https://www.enchor.us) and
  download a chart straight into your library. It's playable immediately.
- **`A` — add folder:** point the game at another folder (e.g. an existing
  Clone Hero `Songs` directory). It's scanned *on top of* the bundled songs and
  remembered across launches.
- **`F` — open songs folder:** reveal `songs/` in Finder/Explorer, so you can
  drag downloads in.
- **`R` — rescan:** pick up newly-added songs without restarting.

### Extra song folders

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
- **rfd** — native "add folder" picker; **ureq** — Chorus Encore search/download
