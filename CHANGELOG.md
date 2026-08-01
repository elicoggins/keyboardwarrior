# Changelog

Notable changes to Keyboard Warrior, newest first. Versions follow
[semantic versioning](https://semver.org): the patch digit is fixes, the minor
digit adds things, the major digit is reserved for a release that changes how
saved settings, scores or song libraries work.

## 1.0.2 — 2026-08-01

A maintenance release. No gameplay, settings or song-library changes — an
existing library and your saved settings carry over untouched.

- **Latency calibration reads your setup more reliably.** The offset is now a
  trimmed average rather than a bare median, so one missed beat no longer drags
  the result, and tapping the same run twice lands in the same place. Taps that
  arrive *before* the tick they'd be blamed on are ignored — a sound can't reach
  you early, and treating anticipation as negative hardware latency was what
  made a retry come back with a different answer. Key repeat and double presses
  no longer count as extra taps either.
- The calibration screen says what it measured (`measured delay 92 ms`) instead
  of a bare signed offset, explains that a delay is normal and compensated for,
  and asks for eight taps rather than four. Hand-nudging with the arrow keys is
  unchanged, negative values included.
- **Fixed: the F1 performance overlay covered the lanes.** On a song it reached
  across two of the four — the readout you turn on to find out why a run feels
  bad was hiding the notes. It's now stacked in the empty column beside the
  highway, and reports memory use alongside the frame stats, with the session
  peak next to each figure.
- **Fixed: the browser demo could render unevenly on older Macs in Chrome**,
  even with plenty of frame time to spare. The demo is also a smaller, cheaper
  build than before.
- Star power's reverb now runs only while there's something to hear, and rings
  out as it always did. Less work in the mixer during ordinary play.

## 1.0.1 — 2026-07-31

A maintenance release. No gameplay, settings or song-library changes — an
existing library and your saved settings carry over untouched.

- **Much lower memory use.** Text is now drawn from a small prerendered glyph
  atlas instead of one built at runtime, which takes the game from roughly
  335 MB down to 117 MB sitting on the menu. Machines with little memory to
  spare should feel it most, the browser demo included.
- The browser demo and the Linux build no longer wait on the graphics driver to
  finish every single frame. That wait was pure overhead on a double-buffered
  display and cost the most on older or integrated graphics.
- **Fixed: typing quickly into the song search box or the folder prompt could
  transpose two letters.** Only affected typed *text entry*; hitting notes
  during a song was never affected.

## 1.0.0 — 2026-07-30

First public release.

- Rhythm typing gameplay: notes fall down the lanes, type the letter on each one
  as it crosses the strike line. Streaks build a multiplier; missed notes duck
  the lead guitar out of the mix.
- **Practice mode** — pick a start and end section, loop the span with a
  count-in, and run it anywhere from 25% to 200% speed.
- **Your own songs** — reads `.sng` files and unpacked song folders
  (`notes.mid` / `notes.chart`, `song.ini`, `.opus` / `.ogg` / `.mp3` / `.wav`
  stems). Whole song packs go in as they came and are scanned recursively.
  Point the game at a library anywhere on disk by typing its path, with **TAB**
  completing folder names as you go.
- **Library browsing** — sort by title, collection, artist, album, decade or
  genre, with a hold-SPACE jump popup for crossing a long library a group at a
  time.
- **Chorus Encore search** — find and download charts without leaving the game.
- **Latency calibration**, so timing is judged fairly on your display and audio
  setup.
- Six freely-licensed songs bundled, credited in the
  [README](README.md#bundled-music).
- Downloads for macOS (one universal build for Apple Silicon and Intel),
  Windows x86-64 and Linux x86-64.
