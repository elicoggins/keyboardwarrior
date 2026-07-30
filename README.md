# Keyboard Warrior

A rhythm typing game.
Built in Rust, inspired by the wonderful [Clone Hero](https://clonehero.net).

**[▶ Play the demo in your browser](https://elicoggins.github.io/keyboardwarrior/)**

## Download

Grab the latest build from the [Releases page](../../releases).

| Platform | File |
| --- | --- |
| macOS (Apple Silicon **and** Intel) | `KeyboardWarrior-<version>-macOS-universal.zip` |
| Windows (x86-64) | `KeyboardWarrior-<version>-windows-x86_64.zip` |
| Linux (x86-64) | `KeyboardWarrior-<version>-linux-x86_64.tar.gz` |

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
strike line. Longer streaks build your multiplier.

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

Or point the app right at your existing library.

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

## Terms

Keyboard Warrior is © 2026 Elijah Coggins. All rights reserved.

The full terms are in [LICENSE.md](LICENSE.md), and the third-party open-source
components the game is built on are listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

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
