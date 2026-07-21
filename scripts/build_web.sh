#!/bin/sh
# Build the browser demo into web/dist — a self-contained static site.
#
#   scripts/build_web.sh          build
#   scripts/build_web.sh serve    build, then serve on localhost:8000
#
# The demo song asset (web/songs/Code Monkey.sng) is produced separately by
# scripts/pack_demo_song.py and committed, so a plain checkout can build.
#
# If the personal site checkout is present, the freshly built assets are also
# mirrored into its Vite `public/keyboardwarrior/` dir — the location the site
# actually serves (the repo-root keyboardwarrior/index.html is only the Vite
# entry point). Override the site location with KW_SITE_DIR; the sync is
# skipped silently when the dir isn't there, so a plain checkout or CI still
# builds. Commit + push in the site repo to deploy.
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

# Mirror the built demo into the personal site so a `git push` there ships the
# latest wasm. Copies everything web/dist produces except index.html — the site
# keeps its own (the portfolio-styled keyboardwarrior/index.html entry).
SITE_PUB="${KW_SITE_DIR:-$HOME/Documents/Code/personal}/public/keyboardwarrior"
if [ -d "$SITE_PUB" ]; then
    cp web/dist/keyboardwarrior.wasm web/dist/kw_audio.js web/dist/mq_js_bundle.js "$SITE_PUB/"
    if [ -f "web/dist/songs/Code Monkey.sng" ]; then
        mkdir -p "$SITE_PUB/songs"
        cp "web/dist/songs/Code Monkey.sng" "$SITE_PUB/songs/"
    fi
    echo "synced demo → $SITE_PUB"
else
    echo "personal site not found at $SITE_PUB — skipping sync (set KW_SITE_DIR to enable)"
fi

if [ "$1" = "serve" ]; then
    echo "serving on http://localhost:8000"
    python3 -m http.server 8000 -d web/dist
fi
