# Changelog

Notable changes to Keyboard Warrior, newest first. Versions follow
[semantic versioning](https://semver.org): the patch digit is fixes, the minor
digit adds things, the major digit is reserved for a release that changes how
saved settings, scores or song libraries work.

## 1.3.0 — 2026-08-30

- **Word Algorithm** - spent lots of time building out a more sophisticated word selection algorithm.
  The over reliance on 8 letter words has been toned down, and word selection now accounts for ergonomics,
  lane distribution, and musical flow. Impact lyrics have also been added - a few times a song you'll see a lyric
  appear to type (if lyrics are available on chart)
- **Highway options** now include strike-line position, note size, center gap,
  center guides, word size, and the number of visible word rows, all reflected
  in the live preview. This gives users much more control and the ability to move
  words higher up on the screen without blocking falling notes to ease vision.
- **Long empty sections are skippable.** Hold SPACE when the prompt appears and
  a 4-3-2-1 count brings you back in before the next note.
- **Enhanced results screen** Every judgement is split into
  EARLY and LATE counts alongside an average timing offset. When a run exposes
  a consistent offset, press A to apply the suggested calibration adjustment.
- Turn on new 'timing labels' option to replace GREAT and GOOD feedback with EARLY, LATE, VERY EARLY,
  and VERY LATE mid song.
- Space bar to preview songs on Download screen

## 1.2.0 — 2026-08-21

User suggested features:

- Adjustable words module: Options now have control over height and opacity of the words module.
- CUSTOM highway speed setting lets you pick anywhere from 0.5x to 5x normal speed. setting is sticky.
- Highway options now feature a highway preview that updates in real time. press P to see a fullscreen preview.
- Layout support added for QWERTY, AZERTY, DVORAK, COLEMAK, QWERTZ in options.
- Fixed a bug where all keyboards would register notes as if they were QWERTY.

Also:

- Updated App icon to monogram and improved title font.
- Rebranded the old DFJK mode to TAP mode and removed the "words."
- Increased word pool.

## 1.1.0 — 2026-08-11

- Ability to enable/disable song directories without deleting them from config
- Lots of visual enhancements in menus / score screen
- Fixed a bug in the browser demo that resulted in missed inputs
- Fixed: deleting a song now returns to correct slot in song list
- Fixed: the whammy ripple looked frozen at higher scroll speeds
- Losing an audio device mid-song now pauses and offers a retry instead of
  ending the run
- Settings and scores survive an interrupted or corrupted save
- Library rescans run in the background and keep your songs on screen
- Large internal rework of the codebase - no gameplay changes intended

## 1.0.4 — 2026-08-04

- Updated CPAL to quiet a message on mac requesting mic permissions
- Toast message when a new version is available to download
- Auto pause game on focus loss
- Scores tracked in WASM demo
- High scores tracked per charter
- F1 menu includes more metrics

## 1.0.3 — 2026-08-02

- Fixed a graphical quirk with note fill size
- Persistent settings in WASM demo using localStorage
- Scaffolding for creating gameplay trailers

## 1.0.2 — 2026-08-01

Improved Latency calibration - less readings in the future

Also in this release:

- Performance overlay displays RAM and total footprint
- Miscellaneous browser demo optimizations

## 1.0.1 — 2026-07-31

Text generation is now handled by a small prerendered glyph atlas instead of being built at runtime for a couple reasons:

- 218 mb freed up from memory allocation
- ~30% less text jitter
- 32x less data pushed to GPU on startup

## 1.0.0 — 2026-07-30

A rhythm typing game built in Rust.

Copying off the homework of the wonderful projects [Clone Hero](https://clonehero.net) and [YARG](https://yarg.in/).
Free, lightweight, and compatible with your existing chart library.

**[▶ Check out the browser demo](https://elicoggins.github.io/keyboardwarrior/)**
