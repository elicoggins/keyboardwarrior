# Changelog

Notable changes to Keyboard Warrior, newest first. Versions follow
[semantic versioning](https://semver.org): the patch digit is fixes, the minor
digit adds things, the major digit is reserved for a release that changes how
saved settings, scores or song libraries work.

## 1.1.0 — 2026-08-11

- **Song folders can be switched off without being thrown out.** A folder in the
  song-folders screen now takes SPACE to stop scanning it and DEL to drop it
  from the list entirely. Neither touches a single file on disk; the difference
  is that a switched-off folder keeps its row, so it's the one you can switch
  back on. Useful if you keep a big pack around but don't want it crowding the
  wheel every session.
- **Fixed: in the browser, notes could die as a MISS you had no way to prevent.**
  A keystroke tells the game two things — which character you typed, and that
  you pressed the key rather than held it — and browsers deliver those as two
  separate events that don't have to arrive in the same frame. When they split,
  the character looked like key-repeat and was thrown away: the note was never
  judged and simply missed. The two halves are now paired across frames.
- **Fixed: deleting a song jumped the cursor somewhere unrelated.** The wheel
  went back to the slot number the deleted song had held, which after the
  rescan belongs to a different song. It now lands on the song that was sitting
  next to the one you deleted.
- **Fixed: the whammy ripple looked frozen at higher scroll speeds.** The wave
  travelled at a fixed rate while the highway scrolled at whatever the speed
  setting chose, so at TURBO the two nearly matched and the ripple appeared to
  sit still on the tail. It's now pinned to the highway's scroll at every speed.
- **Losing an audio device mid-song no longer ends the run.** Unplugging
  headphones pauses cleanly and offers a retry instead of leaving the song
  playing into nothing.
- **Settings and scores survive a corrupted file.** They're written so an
  interrupted save can't leave a half-file behind, and a damaged one falls back
  to the last good copy rather than starting you over. A file too broken to
  read is kept aside rather than silently replaced.
- Rescans — at startup, after a delete, and after a download — happen in the
  background and keep the previous library on screen until the new one is
  ready, instead of emptying the wheel while they run.
- Damaged or hostile `.sng` files are rejected on inspection rather than part
  way through loading.

## 1.0.4 — 2026-08-04

- **Fixed: macOS asked for microphone permission on first launch.** The game
  has never recorded anything — the audio library it uses was enumerating input
  devices alongside output ones, and macOS asks about the microphone the moment
  anything looks at one. Upgrading past that behaviour means the prompt is gone.
- **The game now tells you when there's a newer version.** It asks the releases
  page what the current version is once at launch and parks a small notice in
  the corner of the menu if you're behind. Dismiss it and it stays gone for that
  release. This and the Chorus Encore search are the only times the game talks
  to the network.
- **A run pauses when you switch away from the window.** Alt-tabbing mid-song
  used to leave the notes falling without you.
- **High scores are kept in the browser demo.** They used to be forgotten when
  you closed the tab.
- **Scores are tracked per charter.** Two charts of the same song by different
  charters no longer share one personal best. Bests you've already set carry
  over to every matching chart, so nothing is lost.
- **The mouse cursor gets out of the way** after a couple of seconds idle in the
  menus, and after one during a song.
- Song libraries scan faster, which is most noticeable if yours is large.
- The F1 performance overlay reports more: p95 and p99 frame times, input
  latency, game CPU time, and where the frame actually went.
- The Chorus Encore search identifies itself as what it is rather than
  imitating a browser, and backs off when the server asks it to.
- **The terms of use now ship inside the download** rather than only living on
  this page.

## 1.0.3 — 2026-08-02

- **Fixed a graphical quirk with note fill size.**
- **Persistent settings in browser demo.** Options are saved to the
  browser's `localStorage`, so the demo comes back the way you left it instead
  of starting from defaults on every reload.
- Scaffolding for capturing and rendering gameplay trailers. Tooling for
  building footage, not a change to the game itself.

## 1.0.2 — 2026-08-01

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
