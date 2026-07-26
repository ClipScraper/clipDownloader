#[cfg(target_os = "macos")]
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Root directory that contains a Chromium-family browser's profile folders
/// (each profile holds a `Cookies` or `Network/Cookies` database).
///
/// Locations are OS-specific:
/// - Windows: `%LOCALAPPDATA%\<Vendor>\User Data`
/// - macOS:   `~/Library/Application Support/<Vendor>`
/// - Linux:   `~/.config/<Vendor>`
fn chromium_user_data_root(browser: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let local = dirs::data_local_dir()?;
        let rel = match browser {
            "brave" => "BraveSoftware/Brave-Browser/User Data",
            "chrome" => "Google/Chrome/User Data",
            "edge" => "Microsoft/Edge/User Data",
            "chromium" => "Chromium/User Data",
            _ => return None,
        };
        Some(local.join(rel))
    }
    #[cfg(target_os = "macos")]
    {
        let base = home().join("Library/Application Support");
        let rel = match browser {
            "brave" => "BraveSoftware/Brave-Browser",
            "chrome" => "Google/Chrome",
            "edge" => "Microsoft Edge",
            "chromium" => "Chromium",
            _ => return None,
        };
        Some(base.join(rel))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let base = dirs::config_dir()?;
        let rel = match browser {
            "brave" => "BraveSoftware/Brave-Browser",
            "chrome" => "google-chrome",
            "edge" => "microsoft-edge",
            "chromium" => "chromium",
            _ => return None,
        };
        Some(base.join(rel))
    }
}

/// Directory that contains Firefox-family profile folders.
fn firefox_profiles_root(browser: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let roaming = dirs::config_dir()?;
        let rel = match browser {
            "firefox" => "Mozilla/Firefox/Profiles",
            "librewolf" => "librewolf/Profiles",
            _ => return None,
        };
        Some(roaming.join(rel))
    }
    #[cfg(target_os = "macos")]
    {
        let base = home().join("Library/Application Support");
        let rel = match browser {
            "firefox" => "Firefox/Profiles",
            "librewolf" => "librewolf/Profiles",
            _ => return None,
        };
        Some(base.join(rel))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let rel = match browser {
            "firefox" => ".mozilla/firefox",
            "librewolf" => ".librewolf",
            _ => return None,
        };
        Some(home().join(rel))
    }
}

#[cfg(target_os = "macos")]
fn safari_cookie_path() -> PathBuf {
    home().join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies")
}

/// A Chromium profile stores cookies at either `<profile>/Network/Cookies`
/// (current) or `<profile>/Cookies` (legacy). Return true if either exists.
fn chromium_profile_has_cookies(profile: &Path) -> bool {
    profile.join("Network").join("Cookies").exists() || profile.join("Cookies").exists()
}

fn chromium_profile_args(browser: &str, root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let profile_name = entry.file_name().to_string_lossy().to_string();
        if profile_name != "Default" && !profile_name.starts_with("Profile ") {
            continue;
        }
        if !chromium_profile_has_cookies(&entry.path()) {
            continue;
        }
        let label = if profile_name == "Default" {
            browser.to_string()
        } else {
            format!("{browser} ({profile_name})")
        };
        out.push((label, format!("{browser}:{profile_name}")));
    }
    out
}

fn firefox_profile_args(root: &Path, browser_keyword: &str, browser_label: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let profile_dir = entry.path();
        if !profile_dir.join("cookies.sqlite").exists() {
            continue;
        }
        let profile_name = entry.file_name().to_string_lossy().to_string();
        out.push((format!("{browser_label} ({profile_name})"), format!("{browser_keyword}:{}", profile_dir.display())));
    }
    out
}

// Test-read Safari cookie file to avoid permission errors later.
#[cfg(target_os = "macos")]
fn safari_cookie_readable() -> bool {
    let p = safari_cookie_path();
    if p.exists() {
        if let Ok(mut f) = File::open(&p) {
            let mut buf = [0u8; 1];
            return f.read(&mut buf).is_ok();
        }
    }
    false
}

// Return (browser_label, cookies-from-browser arg) only for usable browsers.
pub fn installed_browsers() -> Vec<(String, String)> {
    let mut chromium = Vec::new();
    for browser in ["brave", "chrome", "edge", "chromium"] {
        if let Some(root) = chromium_user_data_root(browser) {
            if root.exists() {
                chromium.extend(chromium_profile_args(browser, &root));
            }
        }
    }

    let mut firefox = Vec::new();
    if let Some(root) = firefox_profiles_root("firefox") {
        if root.exists() {
            firefox.extend(firefox_profile_args(&root, "firefox", "firefox"));
        }
    }
    if let Some(root) = firefox_profiles_root("librewolf") {
        if root.exists() {
            firefox.extend(firefox_profile_args(&root, "librewolf", "librewolf"));
        }
    }

    let mut v = Vec::new();
    // On Windows, Chromium cookies are protected by App-Bound Encryption
    // (yt-dlp #10927) and almost always fail to decrypt, so try Firefox first
    // to avoid many slow, noisy failures per item. Elsewhere Chromium works,
    // so keep it first.
    #[cfg(target_os = "windows")]
    {
        v.extend(firefox);
        v.extend(chromium);
    }
    #[cfg(not(target_os = "windows"))]
    {
        v.extend(chromium);
        v.extend(firefox);
    }

    // Safari (macOS only), guarded by a readability probe.
    #[cfg(target_os = "macos")]
    {
        if safari_cookie_readable() {
            v.push(("safari".to_string(), "safari".to_string()));
        }
    }

    v
}
