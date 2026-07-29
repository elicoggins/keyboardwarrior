# Keyboard Warrior

A rhythm typing game — the notes come at you down the lanes, and you type them.
Built in Rust, inspired by the wonderful [Clone Hero](https://clonehero.net).

**[▶ Play the demo in your browser](https://elicoggins.github.io/keyboardwarrior/)**
— no download, six songs, runs anywhere WebAssembly does.

## Download

Grab the latest build from the [Releases page](../../releases).

| Platform | File |
| --- | --- |
| macOS (Apple Silicon) | `KeyboardWarrior-macOS.zip` |
| Linux (x86-64) | `keyboardwarrior-linux.tar.gz` |

Six freely-licensed songs ship with the game, so it plays out of the box.

### macOS: first launch

The app isn't signed with an Apple Developer certificate, so macOS will refuse
to open it on the first try and say it can't verify the developer.

**Right-click the app → Open → Open.** You only have to do this once; after
that it launches normally.

### Linux

Unpack anywhere and run `./keyboardwarrior`. You'll need ALSA available
(`libasound2` on Debian/Ubuntu).

## Playing

Notes fall down the lanes; type the letter shown on each one as it crosses the
strike line. Longer streaks build your multiplier, missed notes duck the lead
guitar out of the mix so you can hear yourself falling apart.

- **ENTER** — start the selected song
- **SHIFT + ENTER** — practice mode: pick a start and end section, loop it,
  and use **LEFT / RIGHT** to run it anywhere from 25% to 200% speed
- **S** — cycle how the library is sorted (title, collection, artist, album,
  decade, genre)
- **Hold SPACE** — jump popup, for moving through a long library a group at a
  time
- **O** — options, including latency calibration
- **ESC** — pause

Calibrate your latency the first time you play (**O** → calibration). Every
display and audio setup has some delay, and the game can't judge your timing
fairly until it knows yours.

## Adding your own songs

Keyboard Warrior reads the same charts as other rhythm games. Drop either form
into your songs folder:

- a **`.sng`** file
- a **song folder** (`notes.mid` / `notes.chart`, `song.ini`, and audio stems
  as `.opus` / `.ogg` / `.mp3` / `.wav`)

Whole packs go in as they came — a folder of folders, any depth, mixed `.sng`
and unpacked songs. The scan recurses, and the pack's top-level folder becomes
a collection heading in the menu. Unzip archives first; that's the one
exception.

Press **O** → **TAB** → **SONG FOLDERS** to open your songs folder, point the
game at a library you already have elsewhere, or download charts from Chorus
Encore without leaving the game.

Your songs live outside the app, so replacing it with a newer build never
touches your library:

| | macOS | Linux |
| --- | --- | --- |
| Your songs | `~/Library/Application Support/keyboardwarrior/songs/` | `~/.local/share/keyboardwarrior/songs/` |
| Settings and scores | `~/Library/Application Support/keyboardwarrior/` | `~/.config/keyboardwarrior/` |

## Bundled music

The songs that ship with the game are used under Creative Commons licenses,
with thanks to the artists and to the charters who built the note charts.

| Song | Artist | License | Chart |
| --- | --- | --- | --- |
| Code Monkey | Jonathan Coulton | [CC BY-NC 3.0](https://creativecommons.org/licenses/by-nc/3.0/) | NoisyPuppet |
| Shop Vac | Jonathan Coulton | [CC BY-NC 3.0](https://creativecommons.org/licenses/by-nc/3.0/) | RockGamer |
| Re: Your Brains | Jonathan Coulton | [CC BY-NC 3.0](https://creativecommons.org/licenses/by-nc/3.0/) | Harmonix |
| Dirtbag | Brad Sucks | [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) | JRabes |
| Certain Death | Brad Sucks | [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) | MB1Nightmare & Greninjo |
| There's Something Wrong | Brad Sucks | [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) | MB1Nightmare & Greninjo |

Music by [Jonathan Coulton](https://www.jonathancoulton.com) and
[Brad Sucks](https://www.bradsucks.net). Charts sourced from
[Chorus Encore](https://www.enchor.us).

The browser demo plays pre-mixed versions of these tracks. Those mixes are
adaptations of the originals; the ones derived from Brad Sucks' ShareAlike
material are themselves available under
[CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).

## Terms

Keyboard Warrior is © 2026 Elijah Coggins. All rights reserved.

The game is free to download and play for personal, non-commercial use. The
bundled music and note charts are not mine to relicense — they remain under the
Creative Commons terms listed above, and several are NonCommercial, so the
packaged game as a whole may not be sold or redistributed commercially.

Third-party open-source components are listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## Bugs

Something broken? [Open an issue](../../issues) — include the version from the
options screen and, if it's a specific song, which one.
