use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

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
        "safari" => Some(h.join(
            "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        )),
        _ => None,
    }
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

// Return (browser_key, cookies-from-browser arg) only for usable browsers.
pub fn installed_browsers() -> Vec<(&'static str, &'static str)> {
    let mut v = Vec::new();
    for (b, arg) in [
        ("brave", "brave:Default"),
        ("chrome", "chrome"),
        ("firefox", "firefox"),
        ("safari", "safari"),
    ] {
        if let Some(p) = cookie_db_path(b) {
            if p.exists() {
                if b == "safari" && !safari_cookie_readable() {
                    // Skip Safari if the cookie store is present but unreadable (no Full Disk Access).
                    continue;
                }
                v.push((b, arg));
            }
        }
    }
    if v.is_empty() {
        // If nothing detectable, still try Brave as a best-guess (matches your setup).
        v.push(("brave", "brave:Default"));
    }
    v
}
