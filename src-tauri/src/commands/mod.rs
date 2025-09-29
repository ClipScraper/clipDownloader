use crate::{
    download::{self},
    settings,
    utils::{self},
};
use std::{fs as std_fs, path::PathBuf};
use tauri::{Emitter, Manager};

#[derive(serde::Serialize, Clone)]
struct DownloadResult {
    success: bool,
    message: String,
}

fn emit_status(window: &tauri::WebviewWindow, ok: bool, msg: impl Into<String>) {
    let _ = window.emit(
        "download-status",
        DownloadResult {
            success: ok,
            message: msg.into(),
        },
    );
}

#[tauri::command]
pub async fn download_url(app: tauri::AppHandle, url: String) {
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
        emit_status(
            &window,
            false,
            format!("Starting download for {}...", processed_url),
        );

        tokio::spawn({
            let window = window.clone();
            let processed_url = processed_url.clone();

            async move {
                // 1) Load download root from settings.json (no hard-coded path!)
                let s = settings::load_settings();
                let download_root = PathBuf::from(s.download_directory.clone());
                if let Err(e) = std_fs::create_dir_all(&download_root) {
                    emit_status(&window, false, format!("Failed to create download dir: {e}"));
                    return;
                }

                // 2) Compute site for messages and yt-dlp subdir
                let site = if processed_url.contains("instagram.com") {
                    "instagram"
                } else if processed_url.contains("tiktok.com") {
                    "tiktok"
                } else if processed_url.contains("youtube.com") || processed_url.contains("youtu.be")
                {
                    "youtube"
                } else {
                    "other"
                };

                // For yt-dlp we still save into "<root>/<site>/..."
                let yt_out_dir = download_root.join(site);
                let _ = std_fs::create_dir_all(&yt_out_dir);

                let is_ig = utils::url::is_instagram_post(&processed_url);
                let is_tt_photo = utils::url::is_tiktok_photo(&processed_url);
                let wants_images = is_ig || is_tt_photo;

                let browsers = utils::os::installed_browsers();

                for (browser, cookie_arg) in &browsers {
                    emit_status(&window, false, format!("Trying {}...", browser));

                    if wants_images {
                        // 3) Prefer gallery-dl and point it at the *root* directory
                        //    so it will create "<root>/<platform>/<username>/..."
                        match download::image::run_gallery_dl(&download_root, &processed_url, cookie_arg) {
                            Ok(output) if output.status.success() => {
                                // Message points at "<root>/<site>"
                                let site_dir = download_root.join(site);
                                emit_status(
                                    &window,
                                    true,
                                    format!("Saved images under {}", site_dir.display()),
                                );
                                println!(
                                    "[tauri] gallery-dl ok with {browser}\nstdout:\n{}",
                                    String::from_utf8_lossy(&output.stdout)
                                );
                                return;
                            }
                            Ok(output) => {
                                eprintln!(
                                    "[tauri] gallery-dl failed with {browser}\nstderr:\n{}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
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
                    match download::video::run_yt_dlp(&yt_out_dir, cookie_arg, &processed_url, is_ig) {
                        Ok(out) if out.status.success() => {
                            emit_status(&window, true, format!("Saved to {}", yt_out_dir.display()));
                            println!(
                                "[tauri] yt-dlp ok with {browser}\nstdout:\n{}",
                                String::from_utf8_lossy(&out.stdout)
                            );
                            return;
                        }
                        Ok(out) => {
                            eprintln!(
                                "[tauri] yt-dlp failed with browser: {browser}\nstderr:\n{}",
                                String::from_utf8_lossy(&out.stderr)
                            );
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
                    emit_status(
                        &window,
                        false,
                        "Failed to download with any available browser's cookies.",
                    );
                }
            }
        });
    } else {
        eprintln!("Could not get main window.");
    }
}

#[tauri::command]
pub async fn load_settings() -> settings::Settings {
    settings::load_settings()
}

#[tauri::command]
pub async fn save_settings(settings: settings::Settings) -> Result<(), String> {
    settings::save_settings(&settings)
}

#[tauri::command]
pub async fn pick_csv_and_read(app: tauri::AppHandle) -> Result<String, String> {
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
        FilePath::Path(path_buf) => std_fs::read_to_string(path_buf).map_err(|e| e.to_string()),
        FilePath::Url(url) => Err(format!("Unsupported URL selection: {url}")),
    }
}

#[tauri::command]
pub async fn read_csv_from_path(path: String) -> Result<String, String> {
    println!("[tauri] read_csv_from_path: {}", path);
    std_fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_directory(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();

    if let Some(folder_path) = picked {
        let path_str = match folder_path {
            tauri_plugin_dialog::FilePath::Path(buf) => buf.to_string_lossy().to_string(),
            tauri_plugin_dialog::FilePath::Url(url) => {
                url.to_file_path().unwrap().to_string_lossy().to_string()
            }
        };
        Ok(path_str)
    } else {
        Err("No directory selected".into())
    }
}

#[tauri::command]
pub async fn open_directory(path: String) -> Result<(), String> {
    use std::process::Command;
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
