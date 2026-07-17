#!/bin/sh
# Build the browser demo into web/dist — a self-contained static site.
#
#   scripts/build_web.sh          build
#   scripts/build_web.sh serve    build, then serve on localhost:8000
#
# The demo song asset (web/songs/Code Monkey.sng) is produced separately by
# scripts/pack_demo_song.py and committed, so a plain checkout can build.
set -e
cd "$(dirname "$0")/.."

cargo build --release --target wasm32-unknown-unknown

rm -rf web/dist
mkdir -p web/dist/songs
cp target/wasm32-unknown-unknown/release/keyboardwarrior.wasm web/dist/
cp web/index.html web/kw_audio.js web/mq_js_bundle.js web/dist/
if [ -f "web/songs/Code Monkey.sng" ]; then
    cp "web/songs/Code Monkey.sng" web/dist/songs/
else
    echo "warning: web/songs/Code Monkey.sng missing — run scripts/pack_demo_song.py" >&2
fi

echo "web/dist ready ($(du -sh web/dist | cut -f1))"
if [ "$1" = "serve" ]; then
    echo "serving on http://localhost:8000"
    python3 -m http.server 8000 -d web/dist
fi
