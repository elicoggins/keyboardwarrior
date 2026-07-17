#!/usr/bin/env python3
"""Build the browser demo's song asset: web/songs/Code Monkey.sng.

The browser build has no libopus (C doesn't cross-compile to macroquad's
wasm target), so the demo ships the same song with vorbis stems. To keep the
download small and the in-browser decode fast, the six non-lead stems are
pre-mixed into one backing track — exactly the sum the game itself computes
at load time (see load_song_full + decode::mix_into) — and normalized with
the same formula as decode::finalize_mix, so the demo's mix is
sample-for-sample the mix the native game plays.

The lead stem stays separate (the game ducks it on misses). For Code Monkey
the charted instrument is guitar, so guitar.opus is the lead.

Requires: opusdec (brew install opus-tools), oggenc (brew install
vorbis-tools), numpy.

Usage: python3 scripts/pack_demo_song.py
"""

import struct
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "songs" / "Code Monkey.sng"
OUT = ROOT / "web" / "songs" / "Code Monkey.sng"
LEAD = "guitar.opus"  # the charted instrument's stem (see module docstring)
VORBIS_QUALITY = "5"  # ~160 kbps — matches the source stems' fidelity

# ---------------------------------------------------------------- sng i/o


def mask_bytes(raw: bytes, mask: bytes) -> bytes:
    """The SNGPKG XOR masking — its own inverse, used for read and write."""
    out = bytearray(raw)
    for i in range(len(out)):
        out[i] ^= mask[i % 16] ^ (i & 0xFF)
    return bytes(out)


def read_sng(path: Path):
    data = path.read_bytes()
    if data[:6] != b"SNGPKG":
        sys.exit(f"{path}: not an SNGPKG container")
    version = data[6:10]
    mask = data[10:26]
    pos = 26
    meta_count = struct.unpack_from("<Q", data, pos + 8)[0]
    pos += 16
    metadata = []
    for _ in range(meta_count):
        (klen,) = struct.unpack_from("<i", data, pos)
        key = data[pos + 4 : pos + 4 + klen]
        pos += 4 + klen
        (vlen,) = struct.unpack_from("<i", data, pos)
        val = data[pos + 4 : pos + 4 + vlen]
        pos += 4 + vlen
        metadata.append((key, val))
    file_count = struct.unpack_from("<Q", data, pos + 8)[0]
    pos += 16
    files = {}
    for _ in range(file_count):
        nlen = data[pos]
        name = data[pos + 1 : pos + 1 + nlen].decode()
        pos += 1 + nlen
        ln, off = struct.unpack_from("<QQ", data, pos)
        pos += 16
        files[name] = mask_bytes(data[off : off + ln], mask)
    return version, mask, metadata, files


def write_sng(path: Path, version: bytes, mask: bytes, metadata, files: dict):
    pairs = b""
    for key, val in metadata:
        pairs += struct.pack("<i", len(key)) + key + struct.pack("<i", len(val)) + val
    meta = struct.pack("<QQ", 8 + len(pairs), len(metadata)) + pairs

    names = list(files)
    index_size = sum(1 + len(n.encode()) + 16 for n in names)
    data_start = 26 + len(meta) + 16 + index_size + 8  # +8: data section length

    index = struct.pack("<QQ", 8 + index_size, len(names))
    blobs = b""
    off = data_start
    for n in names:
        raw = files[n]
        nb = n.encode()
        index += struct.pack("<B", len(nb)) + nb + struct.pack("<QQ", len(raw), off)
        blobs += mask_bytes(raw, mask)
        off += len(raw)

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        b"SNGPKG" + version + mask + meta + index + struct.pack("<Q", len(blobs)) + blobs
    )


# ---------------------------------------------------------------- audio


def decode_opus(opus_bytes: bytes, tmp: Path, name: str) -> tuple[np.ndarray, int]:
    """opus bytes -> (float32 stereo frames [n, 2], sample rate)."""
    src = tmp / name
    dst = tmp / (name + ".wav")
    src.write_bytes(opus_bytes)
    # --rate 48000: opus always decodes at 48 kHz; without this opusdec
    # resamples to the pre-encode rate, which the game's own decoder never
    # does (it hands 48 kHz to its load-time resampler, and so do we).
    subprocess.run(
        ["opusdec", "--quiet", "--float", "--rate", "48000", str(src), str(dst)],
        check=True,
    )
    raw = dst.read_bytes()
    # Minimal RIFF walk: opusdec writes format-3 (IEEE float) wav, which the
    # stdlib wave module refuses.
    assert raw[:4] == b"RIFF" and raw[8:12] == b"WAVE", "not a wav"
    pos = 12
    fmt = None
    frames = None
    while pos + 8 <= len(raw):
        cid = raw[pos : pos + 4]
        (clen,) = struct.unpack_from("<I", raw, pos + 4)
        body = raw[pos + 8 : pos + 8 + clen]
        if cid == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body, 0)
            # WAVE_FORMAT_EXTENSIBLE: the real format code leads the GUID
            if fmt[0] == 0xFFFE:
                (sub,) = struct.unpack_from("<H", body, 24)
                fmt = (sub, *fmt[1:])
        elif cid == b"data":
            assert fmt is not None
            audio_fmt, channels, rate, _, _, bits = fmt
            assert audio_fmt == 3 and bits == 32, f"unexpected wav format {fmt}"
            samples = np.frombuffer(body, dtype="<f4")
            frames = samples.reshape(-1, channels)
            if channels == 1:
                frames = np.repeat(frames, 2, axis=1)
            # copy: frombuffer views are read-only, and mixing mutates
            return frames[:, :2].copy(), rate
        pos += 8 + clen + (clen & 1)
    sys.exit(f"{name}: no data chunk in decoded wav")


def encode_vorbis(frames: np.ndarray, rate: int, tmp: Path, name: str) -> bytes:
    """float32 stereo frames -> ogg-vorbis bytes (via float wav + oggenc)."""
    src = tmp / (name + ".mix.wav")
    dst = tmp / (name + ".ogg")
    body = frames.astype("<f4").tobytes()
    hdr = b"RIFF" + struct.pack("<I", 36 + len(body)) + b"WAVE"
    hdr += b"fmt " + struct.pack("<IHHIIHH", 16, 3, 2, rate, rate * 8, 8, 32)
    hdr += b"data" + struct.pack("<I", len(body))
    src.write_bytes(hdr + body)
    subprocess.run(
        ["oggenc", "--quiet", "-q", VORBIS_QUALITY, "-o", str(dst), str(src)],
        check=True,
    )
    return dst.read_bytes()


def main():
    version, mask, metadata, files = read_sng(SRC)
    stems = [n for n in files if n.endswith(".opus")]
    if LEAD not in stems:
        sys.exit(f"lead stem {LEAD} not found in {sorted(stems)}")

    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        rate = None
        backing = None
        lead = None
        for name in sorted(stems):
            frames, r = decode_opus(files[name], tmp, name)
            rate = rate or r
            assert r == rate, f"{name}: sample rate {r} != {rate}"
            if name == LEAD:
                lead = frames
                print(f"  lead    {name}: {len(frames)} frames")
                continue
            # decode::mix_into — sum, growing to the longest stem
            if backing is None:
                backing = frames.copy()
            else:
                if len(backing) < len(frames):
                    backing = np.pad(backing, ((0, len(frames) - len(backing)), (0, 0)))
                backing[: len(frames)] += frames
            print(f"  backing {name}: {len(frames)} frames")

        # decode::finalize_mix — normalize backing and lead together if the
        # sum would clip, with the lead's on-top contribution estimated the
        # same way the game does
        peak_b = float(np.abs(backing).max())
        peak_l = float(np.abs(lead).max())
        peak = max(peak_b, peak_l + peak_b * 0.2)
        if peak > 1.0:
            k = 0.98 / peak
            backing *= k
            lead *= k
            print(f"  normalized by {k:.4f} (combined peak was {peak:.3f})")

        out_files = {
            "song.ogg": encode_vorbis(backing, rate, tmp, "song"),
            "guitar.ogg": encode_vorbis(lead, rate, tmp, "guitar"),
            "notes.mid": files["notes.mid"],
        }
        if "album.jpg" in files:
            out_files["album.jpg"] = files["album.jpg"]

    write_sng(OUT, version, mask, metadata, out_files)
    total = OUT.stat().st_size
    print(f"wrote {OUT} ({total / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
