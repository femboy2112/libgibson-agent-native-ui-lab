#!/usr/bin/env bash
# The whole local gate. Release mode: HumanMusic is ~50x slower in debug builds.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release -- --test-threads=1
# opt-in measurement for LibGibson #15 (prints, asserts nothing):
#   cargo test --release --test pty_resize_collision -- --ignored --nocapture
