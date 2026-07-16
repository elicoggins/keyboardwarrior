# KEYBOARD WARRIOR

A rhythm typing game — monkeytype meets Guitar Hero, **playing real songs from
real Clone Hero charts**. The band plays the actual recording (as audio stems),
and the charted lead line falls down a 4-lane highway as letter gems: type each
letter as its gem crosses the strike line. Miss, and the lead instrument drops
out of the mix until you find the groove again — just like fumbling a solo in
Guitar Hero. Timing is judged (Perfect / Great / Good / Miss), combos multiply
your score, gold gems build star power, and SPACE unleashes it for 2x.

Chart notes are grouped into phrases at musical rests, and each phrase gets a
**real word** whose length matches its note count — so typing a word is
playing a lick. Lanes map to finger zones on QWERTY (left outer / left inner /
right inner / right outer), so the highway doubles as touch-typing form.

## Run

```sh
cargo run --release
```

Up/Down picks a song, Left/Right picks a difficulty (chart songs use the
charter's own EASY/MEDIUM/HARD/EXPERT reductions), Enter plays. Esc quits a
song, R on the results screen restarts.

## Adding songs — zero conversion

Drop a Clone Hero song into `songs/` and play. Both forms work as-is:

- **A `.sng` file** straight from [Chorus Encore](https://www.enchor.us):
  `curl -o "songs/Name.sng" https://files.enchor.us/<md5>.sng`
- **A song folder** (`notes.mid`/`notes.chart`, `song.ini`, audio stems as
  `.opus`/`.ogg`/`.mp3`/`.wav`)

The game reads the .sng container natively and decodes all stem formats
in-process — no ffmpeg, no scripts, no conversion step.

**Licensing**: most community charts index copyrighted recordings — keep
downloads to songs you own, and never commit audio to git (`songs/` is
gitignored). For a legally clean starter library, install
[Clone Hero](https://clonehero.net) and copy its bundled setlist into
`songs/`: those songs (DragonForce, Lich King, and others) were licensed by
their artists for free distribution with the game. Several artists also
publish their own charts on [Chorus Encore](https://www.enchor.us).

## Audio architecture — why the sync is exact

The game runs its own mixer inside a cpal output callback (`src/audio.rs`).
The callback counts every frame delivered to the audio device, and that
counter **is** the game clock: song stems begin at an exact frame index on a
scheduled timeline, judgement windows are measured against the same counter
(blended with the wall clock only to smooth buffer-sized steps), and even the
built-in song's synth events are scheduled at exact timeline frames. Music
and judgement cannot drift apart because they are the same clock. Ducking
the lead stem on a miss is a smoothed per-sample gain in the same callback.

## Tech

- **Rust + [macroquad](https://macroquad.rs)** for rendering/input (its audio
  feature is unused); text uses its built-in pixel font
- **cpal** — output stream; custom mixer with sample-counter timeline
- **symphonia** — ogg-vorbis / mp3 / wav / flac decoding
- **ogg + libopus** — .opus stems (what Chorus Encore ships)
- **midly** — RB-style `notes.mid` (PART GUITAR / PART BASS, difficulty note
  ranges, star power note 116); text `notes.chart` has a hand-rolled parser
- The parser picks whichever 5-fret track (guitar or bass) enters the song
  first, so intros aren't empty (Seven Nation Army rides its bass track), and
  the matching stem (`guitar` or `bass`/`rhythm`) becomes the duckable lead
- UI ticks, count-in clicks, and the miss thud are synthesized at startup
