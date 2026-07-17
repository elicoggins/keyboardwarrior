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
[songs/README.md](songs/README.md) for licenses).

Drop a Clone Hero song into `songs/` and play. Both forms work as-is:

- **A `.sng` file** straight from [Chorus Encore](https://www.enchor.us):
  `curl -o "songs/Name.sng" https://files.enchor.us/<md5>.sng`
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats.

## Browser demo

A one-song demo (Code Monkey) runs in the browser as a static site — same
gameplay code, same mixer, no whammy bar:

```sh
python3 scripts/pack_demo_song.py   # once: repack the demo song with vorbis stems
scripts/build_web.sh serve          # build web/dist and serve on :8000
```

`web/dist` is self-contained; copy it anywhere static (e.g. GitHub Pages).
How it works: macroquad targets wasm out of the box; the cpal callback's
mixer is shared code, pulled from JS by a ScriptProcessorNode so the audio
clock semantics survive; the song is fetched over HTTP and decoded in-page
(libopus doesn't build for wasm, hence the vorbis repack, pre-mixed with the
game's own mix/normalize math).

## Tech

- **Rust + [macroquad](https://macroquad.rs)** for rendering/input
- **cpal** — output stream
- **symphonia** — ogg-vorbis / mp3 / wav / flac decoding
- **ogg + libopus** — .opus stems
- **midly** — RB-style `notes.mid`
