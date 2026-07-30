# Keyboard Warrior

A rhythm typing game — the notes come at you down the lanes, and you type them.
Built in Rust, inspired by the wonderful [Clone Hero](https://clonehero.net).

**[▶ Play the demo in your browser](https://elicoggins.github.io/keyboardwarrior/)**
— no download, six songs, runs anywhere WebAssembly does.

## Download

Grab the latest build from the [Releases page](../../releases).

| Platform | File |
| --- | --- |
| macOS (Apple Silicon **and** Intel) | `KeyboardWarrior-<version>-macOS-universal.zip` |
| Windows (x86-64) | `KeyboardWarrior-<version>-windows-x86_64.zip` |
| Linux (x86-64) | `KeyboardWarrior-<version>-linux-x86_64.tar.gz` |

Six freely-licensed songs ship with the game, so it plays out of the box, and
there's no installer — unpack it wherever you like.

Every release also carries a `SHA256SUMS.txt`. If you want to check your
download arrived intact, put it beside the archive and run:

```sh
sha256sum --ignore-missing -c SHA256SUMS.txt   # Linux
shasum -a 256 -c SHA256SUMS.txt                # macOS
```

```powershell
Get-FileHash .\KeyboardWarrior-*.zip -Algorithm SHA256   # Windows
```

The game isn't code-signed on macOS or Windows — that means a certificate I'd
have to rent annually, and this is free software. Both systems will warn you
about it the first time. Here's how to get past each.

### macOS: first launch

macOS will refuse to open the app and say it can't verify the developer.

1. Double-click the app. macOS blocks it — click **Done**.
2. Open **System Settings → Privacy & Security**, scroll to the Security
   section, and click **Open Anyway** next to the message about Keyboard
   Warrior.
3. Confirm with **Open Anyway** and authenticate.

You only have to do this once; after that it launches normally.

(On older macOS you could right-click → Open instead. Apple removed that
shortcut in macOS 15 Sequoia, so the trip through System Settings is now the
only way.)

### Windows: first launch

Unzip the folder anywhere and run `keyboardwarrior.exe`. Keep the `songs`
folder next to it — that's where the game looks for the music it ships with.

Windows Defender SmartScreen will show a blue **"Windows protected your PC"**
box. Click **More info**, then **Run anyway**. Once again, only the first time.

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

**add a folder** asks you to type a path, starting at your home folder. **TAB**
completes a folder name as you go — press it again to step into the folder it
just completed — then **ENTER** adds it. Whatever you add is remembered, so you
only do it once per library.

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
| Creepy Doll | Jonathan Coulton | [CC BY-NC 3.0](https://creativecommons.org/licenses/by-nc/3.0/) | RockGamer |
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

The full terms are in [LICENSE.md](LICENSE.md), and the third-party open-source
components the game is built on are listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

The game's source is not public. This repo is the download page.

## Bugs

Something broken? [Open an issue](../../issues/new/choose) — include the version
from the options screen and, if it's a specific song, which one.

If the game **crashed**, there's a log worth attaching. It records what went
wrong even though the window vanished:

| | Path |
| --- | --- |
| macOS | `~/Library/Application Support/keyboardwarrior/crash.log` |
| Linux | `~/.config/keyboardwarrior/crash.log` |
| Windows | `%APPDATA%\keyboardwarrior\crash.log` |

The file only exists if the game has actually crashed, and it holds no personal
information beyond the folder paths in the backtrace.

What changed between versions is in [CHANGELOG.md](CHANGELOG.md).
