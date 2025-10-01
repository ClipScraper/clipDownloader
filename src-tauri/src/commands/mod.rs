use crate::{
    database::{OnDuplicate, Settings},
    download::{self},
    settings,
    utils::{self},
    DownloadState,
};
use chrono::Utc;
use std::{fs as std_fs, path::PathBuf};
use tauri::{Emitter, Manager, State};

/// Parse user_handle, clean_name, and file_path from yt-dlp output
/// Expected format: "user_handle - name [id].ext"
/// Returns (user_handle, clean_name, full_file_path)
fn parse_filename_from_output(output: &str) -> (String, String, String) {
    // Look for the destination filename in yt-dlp output
    for line in output.lines() {
        if line.contains("Destination:") {
            // Extract full path from "Destination: /path/to/filename.ext"
            if let Some(colon_pos) = line.find(':') {
                let full_path = &line[colon_pos + 1..].trim();

                // Extract just the filename part for parsing
                if let Some(last_slash) = full_path.rfind('/') {
                    let filename = &full_path[last_slash + 1..];

                    // Parse: "user_handle - name [id].ext"
                    if let Some(bracket_start) = filename.find('[') {
                        if let Some(_bracket_end) = filename.find(']') {
                            let before_bracket = &filename[..bracket_start].trim();

                            // Split on " - " to get user_handle and name
                            if let Some(dash_pos) = before_bracket.find(" - ") {
                                let user_handle = before_bracket[..dash_pos].trim().to_string();
                                let name = before_bracket[dash_pos + 3..].trim().to_string();

                                return (user_handle, name, full_path.to_string());
                            }
                        }
                    }

                    // Fallback: if we can't parse the format, return the whole filename as name
                    return ("Unknown".to_string(), filename.to_string(), full_path.to_string());
                }
            }
        }
    }

    // If no destination found, return defaults
    ("Unknown".to_string(), "Unknown".to_string(), "".to_string())
}

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
pub async fn download_url(
    app: tauri::AppHandle,
    url: String,
    state: State<'_, DownloadState>,
) -> Result<(), String> {
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

        let state_clone = state.inner().clone();
        let handle = tokio::spawn({
            let window = window.clone();
            let processed_url = processed_url.clone();

            async move {
                // 1) Load download root and settings from settings.json
                let s = settings::load_settings();
                let download_root = PathBuf::from(s.download_directory.clone());
                let on_duplicate = s.on_duplicate.clone();

                if let Err(e) = std_fs::create_dir_all(&download_root) {
                    emit_status(&window, false, format!("Failed to create download dir: {e}"));
                    *state_clone.0.lock().unwrap() = None;
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
                    if wants_images {
                        // 3) Prefer gallery-dl for images; base dir is the *root*.
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

                                // Insert download record into database for image downloads
                                if let Ok(db) = crate::database::Database::new() {
                                    // Parse filename from gallery-dl output
                                    let (user_handle, clean_name, file_path) = parse_filename_from_output(&String::from_utf8_lossy(&output.stdout));

                                    let download = crate::database::Download {
                                        id: None,
                                        platform: if processed_url.contains("instagram.com") {
                                            crate::database::Platform::Instagram
                                        } else if processed_url.contains("tiktok.com") {
                                            crate::database::Platform::Tiktok
                                        } else {
                                            crate::database::Platform::Instagram // Default fallback
                                        },
                                        name: clean_name,
                                        media: crate::database::MediaKind::Image,
                                        user: user_handle,
                                        origin: crate::database::Origin::Manual, // User clicked download button
                                        link: processed_url.clone(),
                                        status: crate::database::DownloadStatus::Done,
                                        path: file_path,
                                        date_added: Utc::now(),
                                        date_downloaded: Some(Utc::now()),
                                    };

                                    if let Err(e) = db.insert_download(&download) {
                                        eprintln!("[tauri] Failed to insert download record: {}", e);
                                    }
                                }

                                *state_clone.0.lock().unwrap() = None;
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

                        // TikTok /photo/ is not supported by yt-dlp → try next browser (cookies).
                        if is_tt_photo {
                            continue;
                        }
                        // IG /p/ can still sometimes be handled by yt-dlp → fall through.
                    }

                    // 4) yt-dlp path (video/general, or IG fallback)
                    let window_clone = window.clone();
                    match download::video::run_yt_dlp_with_progress(
                        &yt_out_dir,
                        cookie_arg,
                        &processed_url,
                        is_ig,
                        &on_duplicate,
                        |progress_line| {
                            emit_status(&window_clone, false, progress_line.to_string());
                        },
                    ) {
                        Ok((true, output)) => {
                            // Decide user message based on policy and yt-dlp output
                            let existed = output.contains("has already been downloaded")
                                || output.contains("[download] Skipping")
                                || output.contains("has already been recorded in the archive");

                            let message = match on_duplicate {
                                OnDuplicate::DoNothing if existed => {
                                    format!("File already exists, skipped (as per settings) in {}", yt_out_dir.display())
                                }
                                OnDuplicate::Overwrite => {
                                    // Even if yt-dlp printed "already downloaded", we forced overwrites,
                                    // so we report a normal save.
                                    format!("Saved to {}", yt_out_dir.display())
                                }
                                OnDuplicate::CreateNew => {
                                    // We chose a free name, so we report a normal save.
                                    format!("Saved to {}", yt_out_dir.display())
                                }
                                _ => {
                                    if existed {
                                        format!("File already exists in {}", yt_out_dir.display())
                                    } else {
                                        format!("Saved to {}", yt_out_dir.display())
                                    }
                                }
                            };

                            println!("[tauri] Emitting completion message: {}", message);
                            emit_status(&window, true, message);
                            println!("[tauri] yt-dlp ok with {browser}");

                            // Extract metadata from filename and insert into database
                            if let Ok(db) = crate::database::Database::new() {
                                // Parse filename from yt-dlp output
                                let (user_handle, clean_name, file_path) = parse_filename_from_output(&output);

                                let download = crate::database::Download {
                                    id: None,
                                    platform: if processed_url.contains("youtube.com") || processed_url.contains("youtu.be") {
                                        crate::database::Platform::Youtube
                                    } else if processed_url.contains("instagram.com") {
                                        crate::database::Platform::Instagram
                                    } else if processed_url.contains("tiktok.com") {
                                        crate::database::Platform::Tiktok
                                    } else {
                                        crate::database::Platform::Youtube // Default fallback
                                    },
                                    name: clean_name,
                                    media: if is_ig || is_tt_photo {
                                        crate::database::MediaKind::Image
                                    } else {
                                        crate::database::MediaKind::Video
                                    },
                                    user: user_handle,
                                    origin: crate::database::Origin::Manual, // User clicked download button
                                    link: processed_url.clone(),
                                    status: crate::database::DownloadStatus::Done,
                                    path: file_path,
                                    date_added: Utc::now(),
                                    date_downloaded: Some(Utc::now()),
                                };

                                if let Err(e) = db.insert_download(&download) {
                                    eprintln!("[tauri] Failed to insert download record: {}", e);
                                }
                            }

                            *state_clone.0.lock().unwrap() = None;
                            return;
                        }
                        Ok((false, output)) => {
                            eprintln!(
                                "[tauri] yt-dlp failed with browser: {browser}\noutput:\n{}",
                                output
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
                *state_clone.0.lock().unwrap() = None;
            }
        });
        *state.0.lock().unwrap() = Some(handle);
    } else {
        eprintln!("Could not get main window.");
    }
    Ok(())
}

#[tauri::command]
pub async fn load_settings() -> Settings {
    settings::load_settings()
}

#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<(), String> {
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

#[tauri::command]
pub async fn cancel_download(state: State<'_, DownloadState>) -> Result<(), String> {
    if let Some(handle) = state.0.lock().unwrap().take() {
        handle.abort();
        println!("[tauri] Download cancelled.");
    }
    Ok(())
}
