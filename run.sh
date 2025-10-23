#!/usr/bin/env bash
set -euo pipefail

# Resolve project root (same dir as this script)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ----------------------------
# Sidecars bootstrap for DEV
# ----------------------------
init_sidecars() {
  local os="$(uname -s || echo Unknown)"
  local triple="$(rustc -vV | sed -n 's/^host: //p')"
  local bin_dir="src-tauri/binaries"
  local res_dir="src-tauri/resources"
  mkdir -p "$bin_dir" "$res_dir"

  echo "➡️  Preparing sidecars for $triple"

  # --- yt-dlp ---
  if [[ "$os" == "Darwin" ]]; then
    if [ ! -f "$bin_dir/yt-dlp-$triple" ]; then
      echo "  • fetching yt-dlp (macOS)"
      curl -fsSL https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos -o "$bin_dir/yt-dlp-$triple"
      chmod +x "$bin_dir/yt-dlp-$triple"
    fi
  elif [[ "$os" == "Linux" ]]; then
    if [ ! -f "$bin_dir/yt-dlp-$triple" ]; then
      echo "  • fetching yt-dlp (Linux)"
      curl -fsSL https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o "$bin_dir/yt-dlp-$triple"
      chmod +x "$bin_dir/yt-dlp-$triple"
    fi
  else
    # Windows users should run run.ps1 (this path is for Git Bash users on Windows)
    if [ ! -f "$bin_dir/yt-dlp-$triple.exe" ]; then
      echo "  • fetching yt-dlp (Windows)"
      curl -fsSL -o "$bin_dir/yt-dlp-$triple.exe" https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe
    fi
  fi

  # --- ffmpeg / ffprobe + unsuffixed copies in resources/ ---
  if [[ "$os" == "Linux" ]]; then
    if [ ! -f "$bin_dir/ffmpeg-$triple" ] || [ ! -f "$bin_dir/ffprobe-$triple" ]; then
      echo "  • fetching FFmpeg static (Linux)"
      sudo apt-get update -y >/dev/null 2>&1 || true
      sudo apt-get install -y xz-utils >/dev/null 2>&1 || true
      curl -fsSL -o ffmpeg.tar.xz https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-linux64-gpl.tar.xz
      tar -xf ffmpeg.tar.xz
      rm -f ffmpeg.tar.xz
      FDIR="$(find . -maxdepth 1 -type d -name 'ffmpeg-*linux64-gpl' | head -n1)"
      cp "$FDIR/bin/ffmpeg" "$bin_dir/ffmpeg-$triple"
      cp "$FDIR/bin/ffprobe" "$bin_dir/ffprobe-$triple"
      chmod +x "$bin_dir/ffmpeg-$triple" "$bin_dir/ffprobe-$triple"
      cp "$FDIR/bin/ffmpeg" "$res_dir/ffmpeg"
      cp "$FDIR/bin/ffprobe" "$res_dir/ffprobe"
      chmod +x "$res_dir/ffmpeg" "$res_dir/ffprobe"
      rm -rf "$FDIR"
    fi
  elif [[ "$os" == "Darwin" ]]; then
    # Prefer system/brew ffmpeg if available
    local ff_bin="$(command -v ffmpeg || true)"
    local fp_bin="$(command -v ffprobe || true)"
    if [ -z "$ff_bin" ] || [ -z "$fp_bin" ]; then
      if command -v brew >/dev/null 2>&1; then
        echo "  • installing FFmpeg via Homebrew"
        brew list ffmpeg >/dev/null 2>&1 || brew install ffmpeg
        ff_bin="$(command -v ffmpeg)"
        fp_bin="$(command -v ffprobe)"
      else
        echo "❌ FFmpeg not found and Homebrew not installed."
        echo "   Please install Homebrew (https://brew.sh) then run: brew install ffmpeg"
        echo "   Or place ffmpeg & ffprobe in your PATH."
        exit 1
      fi
    fi
    [ -f "$bin_dir/ffmpeg-$triple" ]   || cp "$ff_bin" "$bin_dir/ffmpeg-$triple"
    [ -f "$bin_dir/ffprobe-$triple" ] || cp "$fp_bin" "$bin_dir/ffprobe-$triple"
    chmod +x "$bin_dir/ffmpeg-$triple" "$bin_dir/ffprobe-$triple"
    cp "$ff_bin" "$res_dir/ffmpeg"
    cp "$fp_bin" "$res_dir/ffprobe"
    chmod +x "$res_dir/ffmpeg" "$res_dir/ffprobe"
  else
    # Windows: handled in run.ps1
    :
  fi

  # --- gallery-dl onefile via local venv + PyInstaller (avoids PEP 668 issues) ---
  build_gallery_dl_onefile "$os" "$triple" "$bin_dir"
}

build_gallery_dl_onefile() {
  local os="$1"
  local triple="$2"
  local bin_dir="$3"

  # Windows uses run.ps1; skip here.
  if [[ "$os" != "Darwin" && "$os" != "Linux" ]]; then
    return 0
  fi

  if [ -f "$bin_dir/gallery-dl-$triple" ]; then
    return 0
  fi

  echo "  • building gallery-dl onefile in isolated venv"
  if ! command -v python3 >/dev/null 2>&1; then
    echo "❌ Python 3 is required to build gallery-dl sidecar"; exit 1
  fi

  # Create a local venv (inside .sidecars) to avoid system pip
  local work_dir="$SCRIPT_DIR/.sidecars/gallerydl-build"
  local venv_dir="$work_dir/venv"
  rm -rf "$work_dir"
  mkdir -p "$work_dir"
  python3 -m venv "$venv_dir"

  # Pick python/pip from venv
  local py="$venv_dir/bin/python"
  local pip="$venv_dir/bin/pip"

  "$py" -m pip install --upgrade pip >/dev/null
  "$pip" install --upgrade gallery-dl pyinstaller >/dev/null

  # Find gallery-dl's __main__.py inside the venv
  local MAIN
  MAIN="$("$py" - <<'PY'
import gallery_dl, os
print(os.path.join(os.path.dirname(gallery_dl.__file__), "__main__.py"))
PY
)"
  # Build single-file executable
  "$py" -m PyInstaller -F -n gallery-dl "$MAIN" >/dev/null

  # Move artifact to sidecars dir
  if [ -f "$work_dir/dist/gallery-dl" ]; then
    mv "$work_dir/dist/gallery-dl" "$bin_dir/gallery-dl-$triple"
    chmod +x "$bin_dir/gallery-dl-$triple"
  else
    echo "❌ PyInstaller did not produce dist/gallery-dl"; exit 1
  fi

  # Cleanup build junk but keep venv cache for faster rebuilds next time
  rm -rf "$work_dir/build" "$work_dir/dist" "$work_dir"/*.spec
}

# ----------------------------
# Existing platform config, etc
# ----------------------------
OS_NAME="$(uname -s || echo Unknown)"

init_platform_config() {
  local cfg_dir=""
  case "$OS_NAME" in
    Darwin) cfg_dir="$HOME/Library/Application Support" ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      if [ -n "${APPDATA-}" ]; then cfg_dir="$APPDATA"; else cfg_dir="${USERPROFILE-}${USERPROFILE:+/AppData/Roaming}"; fi
      command -v cygpath >/dev/null 2>&1 && cfg_dir="$(cygpath -u "$cfg_dir")"
      ;;
    Linux) cfg_dir="${XDG_CONFIG_HOME:-$HOME/.config}" ;;
    *) cfg_dir="$HOME/.config" ;;
  esac

  local app_dir="$cfg_dir/clip-downloader"
  local settings_json="$app_dir/settings.json"
  local db_file="$app_dir/downloads.db"

  mkdir -p "$app_dir"

  if [ ! -f "$settings_json" ]; then
    local default_download_dir=""
    case "$OS_NAME" in
      Darwin) default_download_dir="$HOME/Downloads" ;;
      MINGW*|MSYS*|CYGWIN*|Windows_NT)
        if [ -n "${USERPROFILE-}" ]; then default_download_dir="$USERPROFILE/Downloads"; else default_download_dir="$HOME/Downloads"; fi
        command -v cygpath >/dev/null 2>&1 && default_download_dir="$(cygpath -u "$default_download_dir")"
        ;;
      *) default_download_dir="$HOME/Downloads" ;;
    esac

    cat > "$settings_json" <<EOF
{
  "id": null,
  "download_directory": "${default_download_dir//\\/\/}",
  "on_duplicate": "CreateNew"
}
EOF
  fi

  [ -f "$db_file" ] || : > "$db_file"
}

# Prepare environment for Rust build output
unset NO_COLOR CARGO_TERM_COLOR || true
export CARGO_TARGET_DIR="$SCRIPT_DIR/target"

# Init config + sidecars
init_platform_config
init_sidecars

# Run Tauri dev from project root (so beforeDevCommand runs trunk serve)
exec cargo tauri dev "$@"
