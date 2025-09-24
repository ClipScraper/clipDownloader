#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/src-tauri"

# Avoid tools choking on unusual color envs and stale cargo target paths
unset NO_COLOR CARGO_TERM_COLOR || true
export CARGO_TARGET_DIR="$SCRIPT_DIR/target"

# Always rebuild frontend assets to reflect latest icons and code
cd "$SCRIPT_DIR"
trunk build --release
cd "$SCRIPT_DIR/src-tauri"

exec cargo tauri dev "$@"


