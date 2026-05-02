use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

// macOS cookie DB locations (adjust for other OSes if needed)
fn cookie_db_path(browser: &str) -> Option<PathBuf> {
    let h = home();
    match browser {
        "brave" => {
            Some(h.join("Library/Application Support/BraveSoftware/Brave-Browser/Default/Cookies"))
        }
        "chrome" => Some(h.join("Library/Application Support/Google/Chrome/Default/Cookies")),
        "firefox" => Some(h.join("Library/Application Support/Firefox/Profiles")),
        "librewolf" => Some(h.join("Library/Application Support/librewolf/Profiles")),
        "safari" => Some(h.join(
            "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        )),
        _ => None,
    }
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
        if !entry.path().join("Cookies").exists() {
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

fn firefox_profile_args(root: &Path, browser_label: &str) -> Vec<(String, String)> {
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
        out.push((
            format!("{browser_label} ({profile_name})"),
            format!("firefox:{}", profile_dir.display()),
        ));
    }
    out
}

// Test-read Safari cookie file to avoid permission errors later.
fn safari_cookie_readable() -> bool {
    if let Some(p) = cookie_db_path("safari") {
        if p.exists() {
            if let Ok(mut f) = File::open(&p) {
                let mut buf = [0u8; 1];
                return f.read(&mut buf).is_ok();
            } else {
                return false;
            }
        }
    }
    false
}

// Return (browser_label, cookies-from-browser arg) only for usable browsers.
pub fn installed_browsers() -> Vec<(String, String)> {
    let mut v = Vec::new();
    if let Some(p) = cookie_db_path("brave") {
        if let Some(root) = p.parent() {
            v.extend(chromium_profile_args("brave", root));
        }
    }
    if let Some(p) = cookie_db_path("chrome") {
        if let Some(root) = p.parent() {
            v.extend(chromium_profile_args("chrome", root));
        }
    }
    if let Some(p) = cookie_db_path("firefox") {
        if p.exists() {
            v.push(("firefox".to_string(), "firefox".to_string()));
        }
    }
    if let Some(p) = cookie_db_path("librewolf") {
        if p.exists() {
            v.extend(firefox_profile_args(&p, "librewolf"));
        }
    }
    if let Some(p) = cookie_db_path("safari") {
        if p.exists() && safari_cookie_readable() {
            v.push(("safari".to_string(), "safari".to_string()));
        }
    }
    if v.is_empty() {
        // If nothing detectable, still try Brave default as a best-guess.
        v.push(("brave".to_string(), "brave:Default".to_string()));
    }
    v
}
