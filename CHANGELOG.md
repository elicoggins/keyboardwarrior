# Changelog

Notable changes to Keyboard Warrior, newest first. Versions follow
[semantic versioning](https://semver.org): the patch digit is fixes, the minor
digit adds things, the major digit is reserved for a release that changes how
saved settings, scores or song libraries work.

## 1.0.0 — 2026-07-29

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
