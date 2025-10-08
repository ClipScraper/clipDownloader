Param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Resolve project root (same folder as this script)
$ProjectRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }

function Initialize-PlatformConfig {
  # Windows config base: %APPDATA% (Roaming)
  $cfgDir = if ($env:APPDATA -and $env:APPDATA.Trim()) { $env:APPDATA }
            elseif ($env:USERPROFILE -and $env:USERPROFILE.Trim()) { Join-Path $env:USERPROFILE 'AppData\Roaming' }
            else { Join-Path $env:USERPROFILE 'AppData\Roaming' }

  $appDir = Join-Path $cfgDir 'clip-downloader'
  $settingsPath = Join-Path $appDir 'settings.json'
  $dbPath = Join-Path $appDir 'downloads.db'

  if (-not (Test-Path -LiteralPath $appDir)) {
    New-Item -ItemType Directory -Path $appDir -Force | Out-Null
  }

  if (-not (Test-Path -LiteralPath $settingsPath)) {
    $defaultDownloadDir = if ($env:USERPROFILE -and $env:USERPROFILE.Trim()) {
      Join-Path $env:USERPROFILE 'Downloads'
    } else {
      Join-Path $env:USERPROFILE 'Downloads'
    }
    $settings = [ordered]@{
      id = $null
      download_directory = $defaultDownloadDir
      on_duplicate = 'CreateNew'
    } | ConvertTo-Json -Depth 5
    $settings | Set-Content -LiteralPath $settingsPath -Encoding UTF8
  }

  if (-not (Test-Path -LiteralPath $dbPath)) {
    New-Item -ItemType File -Path $dbPath -Force | Out-Null
  }
}

# Prepare env for builds
if (Test-Path Env:NO_COLOR) { Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue }
if (Test-Path Env:CARGO_TERM_COLOR) { Remove-Item Env:CARGO_TERM_COLOR -ErrorAction SilentlyContinue }
$env:CARGO_TARGET_DIR = Join-Path $ProjectRoot 'target'

# Initialize Windows config (settings.json + downloads.db)
Initialize-PlatformConfig

# Run Tauri dev from project root (so beforeDevCommand runs trunk serve)
Push-Location $ProjectRoot
try {
  & cargo tauri dev @PassThru
} finally {
  Pop-Location
}


