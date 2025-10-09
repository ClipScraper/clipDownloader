#!/usr/bin/env bash
set -euo pipefail

# Resolve project root (same dir as this script)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Cross-platform: detect OS and set config locations
OS_NAME="$(uname -s || echo Unknown)"

init_platform_config() {
  # Determine config path similar to Rust dirs::config_dir()
  local cfg_dir=""

  case "$OS_NAME" in
    Darwin)
      # macOS: $HOME/Library/Application Support
      cfg_dir="$HOME/Library/Application Support"
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      # Git Bash / MSYS / Cygwin / Windows env
      # Prefer APPDATA (e.g., C:\\Users\\<user>\\AppData\\Roaming)
      if [ -n "${APPDATA-}" ]; then
        cfg_dir="$APPDATA"
      else
        # Fallback to Roaming under USERPROFILE
        cfg_dir="${USERPROFILE-}$([ -n "${USERPROFILE-}" ] && echo "/AppData/Roaming" || echo "")"
      fi
      # Convert to POSIX path if cygpath is available
      if command -v cygpath >/dev/null 2>&1; then
        cfg_dir="$(cygpath -u "$cfg_dir")"
      fi
      ;;
    Linux)
      # Linux: XDG_CONFIG_HOME or ~/.config
      cfg_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
      ;;
    *)
      # Fallback: ~/.config
      cfg_dir="$HOME/.config"
      ;;
  esac

  local app_dir="$cfg_dir/clip-downloader"
  local settings_json="$app_dir/settings.json"
  local db_file="$app_dir/downloads.db"

  mkdir -p "$app_dir"

  # Seed settings.json if missing
  if [ ! -f "$settings_json" ]; then
    # Compute default download directory similar to Rust default
    local default_download_dir=""
    case "$OS_NAME" in
      Darwin)
        default_download_dir="$HOME/Downloads"
        ;;
      MINGW*|MSYS*|CYGWIN*|Windows_NT)
        # Attempt to use Windows Downloads known folder via USERPROFILE
        if [ -n "${USERPROFILE-}" ]; then
          default_download_dir="$USERPROFILE/Downloads"
        else
          default_download_dir="$HOME/Downloads"
        fi
        # Convert to POSIX path if cygpath is available
        if command -v cygpath >/dev/null 2>&1; then
          default_download_dir="$(cygpath -u "$default_download_dir")"
        fi
        ;;
      *)
        default_download_dir="$HOME/Downloads"
        ;;
    esac

    cat > "$settings_json" <<EOF
{
  "id": null,
  "download_directory": "${default_download_dir//\\/\/}",
  "on_duplicate": "CreateNew"
}
EOF
  fi

  # Ensure empty DB file exists so rusqlite can open and migrate
  if [ ! -f "$db_file" ]; then
    : > "$db_file"
  fi
}

# Prepare environment for Rust build output
unset NO_COLOR CARGO_TERM_COLOR || true
export CARGO_TARGET_DIR="$SCRIPT_DIR/target"

# Initialize platform config (settings.json + downloads.db)
init_platform_config

# Run Tauri dev from project root (so beforeDevCommand runs trunk serve)
cd "$SCRIPT_DIR"
exec cargo tauri dev "$@"


