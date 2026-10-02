#!/usr/bin/env bash
# Builds the Tuff Console promo video: screenshots of `tuff console serve --demo`,
# an original backing track, and eased zooms, captions, and an end card.
#
#   scripts/promo/build.sh
#
# Everything is written under target/promo/; the result is
# target/promo/tuff-console-promo.mp4 (1920x1080, 30 fps, AAC audio).
# Requires: cargo, node, python3, ffmpeg, curl, and a network connection on the
# first run (Playwright, Pillow, NumPy, and the Geist and JetBrains Mono fonts).
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/scripts/promo"
work="$root/target/promo"
playwright_version="1.63.0"
mkdir -p "$work/shots" "$work/fonts"
cd "$root"

cargo build -p tuffcli --quiet
version="$(cargo pkgid -p tuffcli)"
version="${version##*@}"

if [ ! -d "$work/node/node_modules/playwright" ]; then
  npm install --silent --prefix "$work/node" "playwright@$playwright_version"
  "$work/node/node_modules/.bin/playwright" install chromium
fi
if [ ! -x "$work/venv/bin/python" ]; then
  python3 -m venv "$work/venv"
  "$work/venv/bin/pip" install --quiet pillow numpy
fi

fetch_font() { # <family> <weight> <file>
  [ -s "$work/fonts/$3" ] && return
  local url
  url="$(curl -fsS "https://fonts.googleapis.com/css2?family=$1:wght@$2" | grep -oE 'https://[^)]+\.ttf' | head -1)"
  curl -fsS -o "$work/fonts/$3" "$url"
}
for weight in 400 500 600 700; do fetch_font Geist "$weight" "Geist-$weight.ttf"; done
fetch_font "JetBrains+Mono" 400 JetBrainsMono-Regular.ttf
fetch_font "JetBrains+Mono" 500 JetBrainsMono-Medium.ttf
fetch_font "JetBrains+Mono" 700 JetBrainsMono-Bold.ttf

PW="$work/node/node_modules/playwright" node "$here/capture.mjs" "$root/target/debug/tuff" "$work/shots"
"$work/venv/bin/python" "$here/music.py" "$work/music.wav" 17
PROMO_DIR="$work" TUFF_VERSION="$version" "$work/venv/bin/python" "$here/render.py" "$work/promo-silent.mp4"
ffmpeg -y -loglevel error -i "$work/promo-silent.mp4" -i "$work/music.wav" \
  -c:v copy -c:a aac -b:a 192k -shortest -movflags +faststart "$work/tuff-console-promo.mp4"
echo "Wrote $work/tuff-console-promo.mp4"
