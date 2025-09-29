mod settings;

use tauri::{Emitter, Manager};
use std::{
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(serde::Serialize, Clone)]
struct DownloadResult {
    success: bool,
    message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers

fn emit_status(window: &tauri::WebviewWindow, ok: bool, msg: impl Into<String>) {
    let _ = window.emit(
        "download-status",
        DownloadResult {
            success: ok,
            message: msg.into(),
        },
    );
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

// macOS cookie DB locations (adjust for other OSes if needed)
fn cookie_db_path(browser: &str) -> Option<PathBuf> {
    let h = home();
    match browser {
        "brave" => Some(h.join("Library/Application Support/BraveSoftware/Brave-Browser/Default/Cookies")),
        "chrome" => Some(h.join("Library/Application Support/Google/Chrome/Default/Cookies")),
        "firefox" => Some(h.join("Library/Application Support/Firefox/Profiles")),
        "safari" => Some(h.join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies")),
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
fn installed_browsers() -> Vec<(&'static str, &'static str)> {
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

fn is_instagram_post(u: &str) -> bool {
    u.contains("instagram.com/") && u.contains("/p/")
}
fn is_tiktok_photo(u: &str) -> bool {
    u.contains("tiktok.com/") && u.contains("/photo/")
}

// Try several ways to invoke gallery-dl (Homebrew path, /usr/local, PATH, and python -m)
fn gallery_dl_candidates() -> Vec<(String, Vec<String>)> {
    vec![
        ("/opt/homebrew/bin/gallery-dl".into(), vec![]),
        ("/usr/local/bin/gallery-dl".into(), vec![]),
        ("gallery-dl".into(), vec![]),
        ("python3".into(), vec!["-m".into(), "gallery_dl".into()]),
    ]
}

// For gallery-dl we pass the *root* download dir (user setting) as base directory.
// That way gallery-dl creates "instagram/<user>/..." or "tiktok/<user>/..." under it,
// avoiding the duplicate "<platform>/<platform>/..." you saw.
fn run_gallery_dl(base_download_dir: &Path, url: &str, cookie_arg: &str) -> std::io::Result<std::process::Output> {
    // NOTE: Do NOT pass "--progress" (not supported in your version).
    // "-d" is an alias for "-o base-directory=..."
    let base_args = vec![
        "--verbose".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "-d".into(), base_download_dir.display().to_string(),
        url.into(),
    ];

    let mut last_err: Option<std::io::Error> = None;
    for (prog, prefix) in gallery_dl_candidates() {
        let mut args = prefix.clone();
        args.extend(base_args.clone());

        match Command::new(&prog).args(&args).output() {
            Ok(out) => return Ok(out),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "gallery-dl not found")))
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands

#[tauri::command]
async fn download_url(app: tauri::AppHandle, url: String) {
    println!("[tauri] download_url called with: {}", url);

    if let Some(window) = app.get_webview_window("main") {
        // Normalize minimally: strip IG query params; DO NOT rewrite TikTok /photo/ → /video/
        let mut processed_url = url.clone();
        if processed_url.contains("instagram.com/p/") {
            if let Some((base, _)) = processed_url.split_once('?') {
                processed_url = base.to_string();
            }
        }

        println!("[tauri] processing url: {}", processed_url);
        emit_status(&window, false, format!("Starting download for {}...", processed_url));

        tokio::spawn({
            let window = window.clone();
            let processed_url = processed_url.clone();

            async move {
                // 1) Load download root from settings.json (no hard-coded path!)
                let s = settings::load_settings();
                let download_root = PathBuf::from(s.download_directory.clone());
                if let Err(e) = fs::create_dir_all(&download_root) {
                    emit_status(&window, false, format!("Failed to create download dir: {e}"));
                    return;
                }

                // 2) Compute site for messages and yt-dlp subdir
                let site = if processed_url.contains("instagram.com") {
                    "instagram"
                } else if processed_url.contains("tiktok.com") {
                    "tiktok"
                } else if processed_url.contains("youtube.com") || processed_url.contains("youtu.be") {
                    "youtube"
                } else {
                    "other"
                };

                // For yt-dlp we still save into "<root>/<site>/..."
                let yt_out_dir = download_root.join(site);
                let _ = fs::create_dir_all(&yt_out_dir);

                let is_ig = is_instagram_post(&processed_url);
                let is_tt_photo = is_tiktok_photo(&processed_url);
                let wants_images = is_ig || is_tt_photo;

                let browsers = installed_browsers();

                for (browser, cookie_arg) in &browsers {
                    emit_status(&window, false, format!("Trying {}...", browser));

                    if wants_images {
                        // 3) Prefer gallery-dl and point it at the *root* directory
                        //    so it will create "<root>/<platform>/<username>/..."
                        match run_gallery_dl(&download_root, &processed_url, cookie_arg) {
                            Ok(output) if output.status.success() => {
                                // Message points at "<root>/<site>"
                                let site_dir = download_root.join(site);
                                emit_status(&window, true, format!("Saved images under {}", site_dir.display()));
                                println!("[tauri] gallery-dl ok with {browser}\nstdout:\n{}", String::from_utf8_lossy(&output.stdout));
                                return;
                            }
                            Ok(output) => {
                                eprintln!("[tauri] gallery-dl failed with {browser}\nstderr:\n{}", String::from_utf8_lossy(&output.stderr));
                            }
                            Err(e) => {
                                eprintln!("[tauri] gallery-dl error with {browser}: {e}");
                            }
                        }

                        // For TikTok /photo/, yt-dlp does not support this URL pattern — skip it.
                        if is_tt_photo {
                            continue; // try next browser; maybe different cookies make gallery-dl succeed
                        }
                        // For Instagram /p/, yt-dlp can sometimes enumerate items; fall through to yt-dlp.
                    }

                    // 4) yt-dlp path (video/general, or IG fallback)
                    let mut args: Vec<String> = vec![
                        "--verbose".into(),
                        "-N".into(), "8".into(),
                        "--cookies-from-browser".into(), (*cookie_arg).into(),
                    ];

                    let template = if is_ig {
                        // IG posts: allow autonumber for carousels; don't hard fail if some items are images
                        args.push("--ignore-no-formats-error".into());
                        format!("{}/%(uploader)s - %(title)s [%(id)s]-%(autonumber)03d.%(ext)s", yt_out_dir.display())
                    } else {
                        // Video sites (TikTok *video*, YouTube...)
                        args.extend(vec![
                            "-f".into(), "bestvideo+bestaudio/best".into(),
                            "--merge-output-format".into(), "mp4".into(),
                        ]);
                        format!("{}/%(uploader)s - %(title)s [%(id)s].%(ext)s", yt_out_dir.display())
                    };

                    args.push("-o".into());
                    args.push(template);
                    args.push(processed_url.clone());

                    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

                    match Command::new("yt-dlp").args(&arg_refs).output() {
                        Ok(out) if out.status.success() => {
                            emit_status(&window, true, format!("Saved to {}", yt_out_dir.display()));
                            println!("[tauri] yt-dlp ok with {browser}\nstdout:\n{}", String::from_utf8_lossy(&out.stdout));
                            return;
                        }
                        Ok(out) => {
                            eprintln!("[tauri] yt-dlp failed with browser: {browser}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr));
                        }
                        Err(e) => {
                            eprintln!("[tauri] Failed to exec yt-dlp for {browser}: {e}");
                        }
                    }
                }

                // If we reach here, everything failed across all browsers.
                if wants_images {
                    emit_status(&window, false, "Failed to fetch images. Ensure `gallery-dl` is installed/up-to-date and your chosen browser is logged in.");
                } else {
                    emit_status(&window, false, "Failed to download with any available browser's cookies.");
                }
            }
        });
    } else {
        eprintln!("Could not get main window.");
    }
}

#[tauri::command]
async fn pick_csv_and_read(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let mut file_builder = app.dialog().file();
    if let Some(home) = dirs::home_dir() {
        file_builder = file_builder.set_directory(home);
    }
    let picked = file_builder
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();

    let Some(file_path) = picked else { return Err("No file selected".into()) };
    match file_path {
        FilePath::Path(path_buf) => fs::read_to_string(path_buf).map_err(|e| e.to_string()),
        FilePath::Url(url) => Err(format!("Unsupported URL selection: {url}")),
    }
}

#[tauri::command]
async fn read_csv_from_path(path: String) -> Result<String, String> {
    println!("[tauri] read_csv_from_path: {}", path);
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_settings() -> settings::Settings {
    settings::load_settings()
}

#[tauri::command]
async fn save_settings(settings: settings::Settings) -> Result<(), String> {
    settings::save_settings(&settings)
}

#[tauri::command]
async fn pick_directory(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();

    if let Some(folder_path) = picked {
        let path_str = match folder_path {
            tauri_plugin_dialog::FilePath::Path(buf) => buf.to_string_lossy().to_string(),
            tauri_plugin_dialog::FilePath::Url(url) => url.to_file_path().unwrap().to_string_lossy().to_string(),
        };
        Ok(path_str)
    } else {
        Err("No directory selected".into())
    }
}

#[tauri::command]
async fn open_directory(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            pick_csv_and_read,
            read_csv_from_path,
            download_url,
            load_settings,
            save_settings,
            pick_directory,
            open_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
