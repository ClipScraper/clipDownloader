use crate::{
    database::{OnDuplicate, Settings},
    download::{self},
    settings,
    utils::{self},
    DownloadState,
};
use chrono::Utc;
use std::{fs as std_fs, path::{Path, PathBuf}};
use tauri::{Emitter, Manager, State};

/// Parse user_handle, clean_name, and file_path from yt-dlp/gallery-dl output
/// Expected format: "user_handle - name [id].ext"
/// Returns (user_handle, clean_name, full_file_path)
fn parse_filename_from_output(output: &str, processed_url: &str) -> (String, String, String) {
    parse_multiple_filenames_from_output(output, processed_url, None)
        .into_iter()
        .next()
        .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string(), "".to_string()))
}

/// Extract Instagram (handle, id) from /reel/… or /p/…
fn ig_handle_and_id(url: &str) -> (Option<String>, Option<String>) {
    if let Some(pos) = url.find("instagram.com/") {
        let rest = &url[pos + "instagram.com/".len()..];
        let parts: Vec<&str> = rest.trim_matches('/').split('/').collect();
        if parts.len() >= 3 {
            let handle = parts[0].to_string();
            let typ = parts[1];
            let id = parts[2].to_string();
            if typ == "reel" || typ == "p" {
                return (Some(handle), Some(id));
            }
        }
    }
    (None, None)
}

/// Parse multiple user_handle, clean_name, and file_path from yt-dlp/gallery-dl output
/// Returns a Vec of (user_handle, clean_name, full_file_path)
///
/// Improvements:
/// - Detect yt-dlp "Destination: …" and merger lines with the final path
/// - Honor explicit `--print after_move:filepath` (bare path lines)
/// - Fallback to IG url for handle (and id for name) when needed
fn parse_multiple_filenames_from_output(
    output: &str,
    processed_url: &str,
    yt_out_dir_hint: Option<&Path>,
) -> Vec<(String, String, String)> {
    use std::path::Path as StdPath;
    let mut results = Vec::new();
    let mut candidate_paths: Vec<String> = Vec::new();

    let dir_hint_str = yt_out_dir_hint
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    for line in output.lines() {
        let trimmed = line.trim();

        // gallery-dl style "# /abs/path/file.ext"
        if trimmed.starts_with('#') && trimmed.len() > 2 {
            let full_path = trimmed[2..].trim().to_string();
            candidate_paths.push(full_path);
            continue;
        }

        // yt-dlp: [download] Destination: /abs/path/file.ext
        if let Some(idx) = trimmed.find("Destination: ") {
            let p = trimmed[idx + "Destination: ".len()..].trim().to_string();
            candidate_paths.push(p);
            continue;
        }

        // yt-dlp: [Merger] Merging formats into "/abs/path/file.ext"
        if trimmed.contains("Merging formats into") {
            if let Some(q1) = trimmed.find('"') {
                if let Some(q2) = trimmed[q1 + 1..].find('"') {
                    let p = &trimmed[q1 + 1..q1 + 1 + q2];
                    candidate_paths.push(p.to_string());
                    continue;
                }
            }
            if let Some(after) = trimmed.split("into").nth(1) {
                candidate_paths.push(after.trim_matches(|c| c == '"' || c == ' ').to_string());
                continue;
            }
        }

        // our explicit --print after_move:filepath / filepath (bare line path)
        let looks_absolute_unix = trimmed.starts_with('/');
        let looks_absolute_win = trimmed.len() > 2
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/');
        if looks_absolute_unix
            || looks_absolute_win
            || (!dir_hint_str.is_empty() && trimmed.starts_with(&dir_hint_str))
        {
            if trimmed.contains('.') {
                candidate_paths.push(trimmed.to_string());
                continue;
            }
        }

        // yt-dlp skip messages (no dir) → join with hint if we have it
        if trimmed.starts_with("[download] ") && trimmed.contains(" has already been downloaded") {
            let name_part = trimmed
                .trim_start_matches("[download] ")
                .split(" has already been downloaded")
                .next()
                .unwrap_or("")
                .trim();
            if !name_part.is_empty() && !dir_hint_str.is_empty() {
                candidate_paths.push(format!("{}/{}", dir_hint_str, name_part));
            }
            continue;
        }
        if trimmed.starts_with("[download] Skipping")
            && trimmed.contains("has already been recorded in the archive")
        {
            if let Some(after) = trimmed.strip_prefix("[download] Skipping ") {
                let fname = after.split(':').next().unwrap_or("").trim();
                if !fname.is_empty() && !dir_hint_str.is_empty() {
                    candidate_paths.push(format!("{}/{}", dir_hint_str, fname));
                }
            }
            continue;
        }
    }

    // Dedup paths (keep first occurrence)
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut unique_paths = Vec::new();
    for p in candidate_paths.into_iter() {
        if seen.insert(p.clone()) {
            unique_paths.push(p);
        }
    }

    // Build results with best-effort metadata
    for full_path in unique_paths.into_iter() {
        let full = StdPath::new(&full_path);
        let filename = full.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let mut clean_name = filename.to_string();
        let mut user_handle = "Unknown".to_string();

        // Parse "uploader - title [id].ext" → clean_name = title
        if let Some(stem) = full.file_stem().and_then(|s| s.to_str()) {
            let s = stem.to_string();
            if let Some(bracket_start) = s.find('[') {
                let before_bracket = s[..bracket_start].trim();
                if let Some(dash_pos) = before_bracket.find(" - ") {
                    clean_name = before_bracket[dash_pos + 3..].trim().to_string();
                } else {
                    clean_name = before_bracket.to_string();
                }
            } else {
                clean_name = s;
            }
        }

        // Prefer IG handle from URL; else try parent folder (gallery-dl)
        if processed_url.contains("instagram.com/") {
            if let (Some(h), _) = ig_handle_and_id(processed_url) {
                user_handle = h;
            }
        }
        if user_handle == "Unknown" {
            if let Some(parent) = full.parent() {
                if let Some(last) = parent.file_name().and_then(|s| s.to_str()) {
                    if last != "instagram" && last != "tiktok" && last != "youtube" {
                        user_handle = last.to_string();
                    } else if let Some(pp) = parent.parent() {
                        if let Some(prev) = pp.file_name().and_then(|s| s.to_str()) {
                            if prev != "instagram" && prev != "tiktok" && prev != "youtube" {
                                user_handle = prev.to_string();
                            }
                        }
                    }
                }
            }
        }

        // ✅ Force IG id as name for /reel/ and /p/
        if processed_url.contains("instagram.com/") {
            if let (_, Some(id)) = ig_handle_and_id(processed_url) {
                clean_name = id;
            }
        }

        results.push((user_handle, clean_name, full_path));
    }

    // If nothing detected (very quiet output), at least fill from URL for IG
    if results.is_empty() {
        let (h, maybe_id) = ig_handle_and_id(processed_url);
        let handle = h.unwrap_or_else(|| "Unknown".to_string());
        let name = if processed_url.contains("instagram.com/") {
            maybe_id.unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };
        results.push((handle, name, String::new()));
    }

    results
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
        if processed_url.contains("instagram.com/") {
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

                                // Insert download record(s) into database for image downloads
                                if let Ok(db) = crate::database::Database::new() {
                                    // Parse multiple filenames from gallery-dl output
                                    let files = parse_multiple_filenames_from_output(
                                        &String::from_utf8_lossy(&output.stdout),
                                        &processed_url,
                                        None,
                                    );

                                    // Generate image set ID if multiple files (carousel)
                                    let image_set_id = if files.len() > 1 {
                                        Some(uuid::Uuid::new_v4().to_string())
                                    } else {
                                        None
                                    };

                                    for (mut user_handle, mut clean_name, mut file_path) in files {
                                        // Fallbacks from URL for IG
                                        if processed_url.contains("instagram.com/") {
                                            if user_handle == "Unknown" {
                                                if let (Some(h), _) = ig_handle_and_id(&processed_url) {
                                                    user_handle = h;
                                                }
                                            }
                                            // ✅ Force name to id for IG /reel/ or /p/
                                            if let (_, Some(id)) = ig_handle_and_id(&processed_url) {
                                                clean_name = id;
                                            }
                                        }
                                        if file_path.is_empty() {
                                            file_path = "unknown_path".to_string();
                                        }

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
                                            image_set_id: image_set_id.clone(),
                                            date_added: Utc::now(),
                                            date_downloaded: Some(Utc::now()),
                                        };

                                        if let Err(e) = db.insert_download(&download) {
                                            eprintln!("[tauri] Failed to insert download record: {}", e);
                                        }
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
                                    format!("Saved to {}", yt_out_dir.display())
                                }
                                OnDuplicate::CreateNew => {
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

                            // Extract metadata from output and insert into database
                            if let Ok(db) = crate::database::Database::new() {
                                let mut files = parse_multiple_filenames_from_output(
                                    &output,
                                    &processed_url,
                                    Some(&yt_out_dir),
                                );

                                // ✅ Force DB name to IG id for reels/posts
                                if processed_url.contains("instagram.com/") {
                                    if let (_, Some(id)) = ig_handle_and_id(&processed_url) {
                                        for f in files.iter_mut() {
                                            f.1 = id.clone();
                                        }
                                    }
                                }

                                // Generate image set ID if multiple files (carousel)
                                let image_set_id = if files.len() > 1 {
                                    Some(uuid::Uuid::new_v4().to_string())
                                } else {
                                    None
                                };

                                for (mut user_handle, clean_name, mut file_path) in files {
                                    // Ensure handle from URL for IG
                                    if processed_url.contains("instagram.com/") && user_handle == "Unknown" {
                                        if let (Some(h), _) = ig_handle_and_id(&processed_url) {
                                            user_handle = h;
                                        }
                                    }

                                    if file_path.is_empty() {
                                        file_path = "unknown_path".to_string();
                                    }

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
                                        image_set_id: image_set_id.clone(),
                                        date_added: Utc::now(),
                                        date_downloaded: Some(Utc::now()),
                                    };

                                    if let Err(e) = db.insert_download(&download) {
                                        eprintln!("[tauri] Failed to insert download record: {}", e);
                                    }
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
