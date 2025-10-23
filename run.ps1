#Requires -Version 5
$ErrorActionPreference = "Stop"
Set-Location -LiteralPath $PSScriptRoot

function Init-Sidecars {
  $triple = & rustc -vV | Select-String -Pattern '^host: ' | ForEach-Object { $_.ToString().Split(':')[1].Trim() }
  $binDir = "src-tauri\binaries"
  $resDir = "src-tauri\resources"
  New-Item -Force -ItemType Directory -Path $binDir | Out-Null
  New-Item -Force -ItemType Directory -Path $resDir | Out-Null
  Write-Host "➡️  Preparing sidecars for $triple"

  # yt-dlp
  if (!(Test-Path "$binDir\yt-dlp-$triple.exe")) {
    Write-Host "  • fetching yt-dlp (Windows)"
    Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe" -OutFile "$binDir\yt-dlp-$triple.exe"
  }

  # ffmpeg/ffprobe (static build)
  if (!(Test-Path "$binDir\ffmpeg-$triple.exe" -and (Test-Path "$binDir\ffprobe-$triple.exe"))) {
    Write-Host "  • fetching FFmpeg static (Windows)"
    $tmpZip = "ffmpeg.zip"
    Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" -OutFile $tmpZip
    tar -xf $tmpZip
    Remove-Item $tmpZip -Force
    $dir = Get-ChildItem -Directory -Filter "ffmpeg-*win64-gpl" | Select-Object -First 1
    Copy-Item "$($dir.FullName)\bin\ffmpeg.exe" "$binDir\ffmpeg-$triple.exe"
    Copy-Item "$($dir.FullName)\bin\ffprobe.exe" "$binDir\ffprobe-$triple.exe"
    Copy-Item "$($dir.FullName)\bin\ffmpeg.exe" "$resDir\ffmpeg.exe"
    Copy-Item "$($dir.FullName)\bin\ffprobe.exe" "$resDir\ffprobe.exe"
    Remove-Item $dir.FullName -Recurse -Force
  } else {
    if (!(Test-Path "$resDir\ffmpeg.exe")) { Copy-Item "$binDir\ffmpeg-$triple.exe" "$resDir\ffmpeg.exe" }
    if (!(Test-Path "$resDir\ffprobe.exe")) { Copy-Item "$binDir\ffprobe-$triple.exe" "$resDir\ffprobe.exe" }
  }

  # gallery-dl onefile
  if (!(Test-Path "$binDir\gallery-dl-$triple.exe")) {
    Write-Host "  • building gallery-dl onefile (requires Python)"
    & py -3 -m pip install --upgrade pip | Out-Null
    & py -3 -m pip install gallery-dl pyinstaller | Out-Null
    $main = py -3 - <<'PY'
import gallery_dl, os
print(os.path.join(os.path.dirname(gallery_dl.__file__), "__main__.py"))
PY
    $main = $main.Trim()
    & py -3 -m PyInstaller -F -n gallery-dl "$main" | Out-Null
    Move-Item "dist\gallery-dl.exe" "$binDir\gallery-dl-$triple.exe" -Force
    Remove-Item -Recurse -Force build, dist, *.spec -ErrorAction SilentlyContinue
  }
}

function Init-Config {
  $cfgDir = if ($env:APPDATA) { $env:APPDATA } else { Join-Path $env:USERPROFILE "AppData\Roaming" }
  $appDir = Join-Path $cfgDir "clip-downloader"
  $settings = Join-Path $appDir "settings.json"
  $db = Join-Path $appDir "downloads.db"
  New-Item -Force -ItemType Directory -Path $appDir | Out-Null

  if (!(Test-Path $settings)) {
    $dl = if ($env:USERPROFILE) { Join-Path $env:USERPROFILE "Downloads" } else { "$HOME\Downloads" }
    @"
{
  "id": null,
  "download_directory": "$dl",
  "on_duplicate": "CreateNew"
}
"@ | Out-File -Encoding UTF8 $settings
  }
  if (!(Test-Path $db)) { New-Item -ItemType File -Path $db | Out-Null }
}

Init-Config
Init-Sidecars

# Run tauri dev (trunk serve will be run by beforeDevCommand)
cargo tauri dev -- $args
