# Keyboard Warrior

A rhythm typing game, copying off the homework of the wonderful project [Clone Hero](https://clonehero.net).

## Run

```sh
cargo run --release
```

## Adding songs — zero conversion

The song library is read from `songs/` at launch — nothing is baked into the
binary, so add or remove songs freely and just restart the game. A few
freely-licensed songs are bundled to play out of the box (see
[songs/README.md](songs/README.md) for licenses); anything else you drop in
stays local and is never committed.

Drop a Clone Hero song into `songs/` and play. Both forms work as-is:

- **A `.sng` file** straight from [Chorus Encore](https://www.enchor.us):
  `curl -o "songs/Name.sng" https://files.enchor.us/<md5>.sng`
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats.

## Tech

- **Rust + [macroquad](https://macroquad.rs)** for rendering/input
- **cpal** — output stream
- **symphonia** — ogg-vorbis / mp3 / wav / flac decoding
- **ogg + libopus** — .opus stems
- **midly** — RB-style `notes.mid`
