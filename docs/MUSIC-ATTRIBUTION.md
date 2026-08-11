# Bundled songs


| Song | Artist | Recorded license claim | Chart credit |
| --- | --- | --- | --- |
| Code Monkey | Jonathan Coulton | CC BY-NC 3.0 | NoisyPuppet |
| Creepy Doll | Jonathan Coulton | CC BY-NC 3.0 | RockGamer |
| Re: Your Brains | Jonathan Coulton | CC BY-NC 3.0 | Harmonix |
| Dirtbag | Brad Sucks | CC BY-SA 3.0 | JRabes |
| Certain Death | Brad Sucks | CC BY-SA 3.0 | MB1Nightmare & Greninjo |
| There's Something Wrong | Brad Sucks | CC BY-SA 3.0 | MB1Nightmare & Greninjo |

These are recorded claims and credits, not completed rights evidence. The
checked source of truth is [`legal/bundled-content.json`](../legal/bundled-content.json),
which pins each `.sng` hash and lists the exact recording and chart evidence
still missing. It also pins every source size and enforces an 85,000,000-byte
total budget (the current set is 77,373,662 bytes). In particular, a recording
license does not establish a chart's redistribution terms. The release workflow
remains blocked until the manifest contains direct evidence and an explicit
review for every bundled work.

These are the defaults — they always stay in the library, and the game refuses
to delete them. `scripts/package.sh` puts exactly the files git tracks in this
folder into the packaged app, so a downloaded build ships them (and this file's
attributions) too. In a source checkout, the root README explains where native
builds keep bundled and user-added songs.

The copies here are the full multi-stem sources. Native packaging uses
`scripts/pack_bundled.py` to sum non-lead stems and re-encode them as one
128 kbps Opus backing track, reducing roughly 77 MB of audio to 48 MB. Browser
packaging decodes these sources, sums non-lead stems, scales the combined mix
when necessary, and re-encodes backing and any lead stem as Vorbis; it also
writes chart-only containers. Those transformations are disclosed in the
checked provenance manifest. Chart bytes themselves are copied unchanged.

To add your own songs, use the in-game song-folders screen (`O` then `TAB`) and
select **add a folder**. In a source checkout you can also drop them into the
root `songs/` directory. Whole packs can be dropped in as they came, subfolders
and all: the scan recurses, and each pack's top folder becomes its collection
in the menu. These bundled songs group under `Bundled` in the menu's collection
sort.
