# Keyboard Warrior

A rhythm typing game built in Rust.

Copying off the homework of the wonderful projects [Clone Hero](https://clonehero.net) and [YARG](https://yarg.in/). 
Free, lightweight, and compatible with your existing chart library. Built-in bridge to [Chorus Encore](https://www.enchor.us/)
to download community charts.

Available on macOS, Windows and Linux, as well as a playable demo:

**[▶ Play in your browser](https://keyboardwarrior.app)**

## Trailer (audio on)
https://github.com/user-attachments/assets/ad2dcd71-fcf9-4518-be31-2fefaadcb1bb


## Gameplay

Keyboard Warrior puts a spin on Guitar Hero-like rhythm games by assigning a letter to each note, forming words. Type the corresponding letter in rhythm to hit the note. **SPACE** unleashes your star power and **SHIFT** is your whammy bar.

- **ENTER** — start the selected song
- **SHIFT + ENTER** — practice mode: pick a start and end section, loop it,
  and use **LEFT / RIGHT** to run it anywhere from 25% to 200% speed
- **S** — cycle how the library is sorted (title, collection, artist, album,
  decade, genre)
- **Hold SPACE** — jump between category sections
- **O** — options
- **ESC** — pause

Calibrate your latency the first time you play (**C** → calibration). Every
display and audio setup has some delay, and the game can't judge your timing
fairly until it knows yours.

If you change your setup (switch to headphones, etc) **O** to recalibrate takes seconds.

## Download

Grab the latest release below or check out the [Releases page](../../releases).

| Platform | File |
| --- | --- |
| macOS (Apple Silicon **and** Intel) | [`KeyboardWarrior-1.1.0-macOS-universal.zip`](https://github.com/elicoggins/keyboardwarrior/releases/download/v1.1.0/KeyboardWarrior-1.1.0-macOS-universal.zip) |
| Windows (x86-64) | [`KeyboardWarrior-1.1.0-windows-x86_64.zip`](https://github.com/elicoggins/keyboardwarrior/releases/download/v1.1.0/KeyboardWarrior-1.1.0-windows-x86_64.zip) |
| Linux (x86-64) | [`KeyboardWarrior-1.1.0-linux-x86_64.tar.gz`](https://github.com/elicoggins/keyboardwarrior/releases/download/v1.1.0/KeyboardWarrior-1.1.0-linux-x86_64.tar.gz) |

### macOS: first launch

macOS will be annoying and refuse to open the app.

1. Download the binary and stick it in your Applications folder.
2. Double-click the app and macOS will block it. Click **Done**.
3. Open **System Settings → Privacy & Security**, scroll to the Security
   section, and click **Open Anyway** next to the message about Keyboard
   Warrior.
4. Confirm with **Open Anyway** and authenticate.

You only have to do this once - after that it launches normally.

### Windows: first launch

Unzip the folder anywhere and run `keyboardwarrior.exe`. Keep the
`bundled_songs` folder next to it — that's where the game looks for the bundled music.

Windows Defender SmartScreen will show a blue **"Windows protected your PC"**
box. Click **More info**, then **Run anyway**. Once again, only the first time.

### Linux

Unpack anywhere and run `./keyboardwarrior`. You'll need ALSA available
(`libasound2` on Debian/Ubuntu). Keep the `bundled_songs` folder next to the
binary.

## Adding songs

Keyboard Warrior ships with a built-in bridge to [Chorus Encore](https://www.enchor.us/) - Search and download community charts from right inside the app.

It reads the same standardized file formats as other popular music rhythm games, which means there's also tons of resources online for sourcing charts yourself. Drop either format into your songs folder:

- a **`.sng`** file
- a **song folder** (`notes.mid` / `notes.chart`, `song.ini`, and audio stems
  as `.opus` / `.ogg` / `.mp3` / `.wav`)

Your songs live outside the app, so replacing it with a newer build never
touches your library:

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Your songs | `~/Library/Application Support/keyboardwarrior/songs/` | `%APPDATA%\keyboardwarrior\songs\` | `~/.local/share/keyboardwarrior/songs/` |
| Settings and scores | `~/Library/Application Support/keyboardwarrior/` | `%APPDATA%\keyboardwarrior\` | `~/.config/keyboardwarrior/` |

Or point the app right at your existing library. Multiple directories supported at one time.

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

Free to download, play and share.

The full terms are in [LICENSE.md](LICENSE.md), and the third-party open-source
components the game is built on are listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). A copy of both ships inside
the download.

This game is capable of two outbound network requests: The first checks for new versions of
the game, and the second is the optional Chorus Encore search and download. Nothing else is 
sent or collected. Songs, scores, and settings are completely local and private.

## Bugs

Something broken? [Open an issue](../../issues/new/choose) — include the version
from the options screen and, if it's a specific song, which one.

If the game **crashed**, there's a log worth attaching.

| | Path |
| --- | --- |
| macOS | `~/Library/Application Support/keyboardwarrior/crash.log` |
| Windows | `%APPDATA%\keyboardwarrior\crash.log` |
| Linux | `~/.config/keyboardwarrior/crash.log` |

What changed between versions is in [CHANGELOG.md](CHANGELOG.md).

## Technology

- **[Rust](https://www.rust-lang.org)** —  distills to single 12 MB binary
- **[macroquad](https://macroquad.rs)** — window, rendering and input
- **[cpal](https://github.com/RustAudio/cpal)** — drives custom mixer
- **[symphonia](https://github.com/pdeljanov/Symphonia)** and **[libopus](https://opus-codec.org)** — stem decoding (opus, ogg/vorbis, mp3, wav, flac)
- **[midly](https://github.com/kovaxis/midly)** — `.mid` chart parsing
- **[WebAssembly](https://webassembly.org)** — web audio swap for WASM demo

Full dependency list and licenses: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
