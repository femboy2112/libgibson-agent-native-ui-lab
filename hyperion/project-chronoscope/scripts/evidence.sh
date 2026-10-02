#!/usr/bin/env bash
# Regenerate every file under docs/evidence/ (takes ~10 minutes with the 60k run).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
B=./target/release/project-chronoscope
$B --evidence=audio  > docs/evidence/audio.md
$B --evidence=matrix > docs/evidence/matrix.md
$B --sustained --frames=12000 --out=docs/evidence/sustained-12000.md >/dev/null
$B --sustained --frames=12000 --audio-every=40 --out=docs/evidence/sustained-12000-audio.md >/dev/null
$B --sustained --frames=60000 --out=docs/evidence/sustained-60000.md >/dev/null
