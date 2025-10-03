use super::event::emit_status;
use super::parse::{ig_handle_and_id, parse_multiple_filenames_from_output};
use crate::database::OnDuplicate;
use chrono::Utc;
use std::{fs as std_fs, path::PathBuf};
use tauri::{Manager, State};

#[tauri::command]
pub async fn download_url(
    app: tauri::AppHandle,
    url: String,
    state: State<'_, crate::DownloadState>,
) -> Result<(), String> {
    println!("[BACKEND][DOWNLOADER] download_url called with: {}", url);

    if let Some(window) = app.get_webview_window("main") {
        // Normalize minimally: strip IG query params
        let mut processed_url = url.clone();
        if processed_url.contains("instagram.com/") {
            if let Some((base, _)) = processed_url.split_once('?') {
                processed_url = base.to_string();
            }
        }

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
                // Load settings
                let s = crate::settings::load_settings();
                let download_root = PathBuf::from(s.download_directory.clone());
                let on_duplicate = s.on_duplicate.clone();

                if let Err(e) = std_fs::create_dir_all(&download_root) {
                    emit_status(&window, false, format!("Failed to create download dir: {e}"));
                    *state_clone.0.lock().unwrap() = None;
                    return;
                }

                // Compute site directory
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

                // yt-dlp outputs into "<root>/<site>/..."
                let yt_out_dir = download_root.join(site);
                let _ = std_fs::create_dir_all(&yt_out_dir);

                let is_ig_images = crate::utils::url::is_instagram_post(&processed_url);
                let is_tt_photo = crate::utils::url::is_tiktok_photo(&processed_url);
                let wants_images = is_ig_images || is_tt_photo;

                let browsers = crate::utils::os::installed_browsers();

                for (browser, cookie_arg) in &browsers {
                    if wants_images {
                        // Prefer gallery-dl for images; base dir is the *root*.
                        match crate::download::image::run_gallery_dl(
                            &download_root,
                            &processed_url,
                            cookie_arg,
                        ) {
                            Ok(output) if output.status.success() => {
                                // Message points at "<root>/<site>"
                                let site_dir = download_root.join(site);
                                emit_status(
                                    &window,
                                    true,
                                    format!("Saved images under {}", site_dir.display()),
                                );
                                // println!("[tauri] gallery-dl ok with {browser}\nstdout:\n{}", String::from_utf8_lossy(&output.stdout));

                                // Insert records
                                if let Ok(db) = crate::database::Database::new() {
                                    let files = parse_multiple_filenames_from_output(
                                        &String::from_utf8_lossy(&output.stdout),
                                        &processed_url,
                                        None,
                                    );

                                    let image_set_id = if files.len() > 1 {
                                        Some(uuid::Uuid::new_v4().to_string())
                                    } else {
                                        None
                                    };

                                    for (mut user_handle, mut clean_name, mut file_path) in files {
                                        if processed_url.contains("instagram.com/") {
                                            if user_handle == "Unknown" {
                                                if let (Some(h), _) =
                                                    ig_handle_and_id(&processed_url)
                                                {
                                                    user_handle = h;
                                                }
                                            }
                                            // Force IG id as name
                                            if let (_, Some(id)) = ig_handle_and_id(&processed_url)
                                            {
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
                                                crate::database::Platform::Instagram
                                            },
                                            name: clean_name,
                                            media: crate::database::MediaKind::Image,
                                            user: user_handle,
                                            origin: crate::database::Origin::Manual,
                                            link: processed_url.clone(),
                                            status: crate::database::DownloadStatus::Done,
                                            path: file_path,
                                            image_set_id: image_set_id.clone(),
                                            date_added: Utc::now(),
                                            date_downloaded: Some(Utc::now()),
                                        };
                                        let _ = db.insert_download(&download);
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

                        if is_tt_photo {
                            continue;
                        }
                    }

                    // yt-dlp path (video/general, or IG fallback)
                    let window_clone = window.clone();
                    match crate::download::video::run_yt_dlp_with_progress(
                        &yt_out_dir,
                        cookie_arg,
                        &processed_url,
                        is_ig_images,
                        &on_duplicate,
                        |progress_line| {
                            emit_status(&window_clone, false, progress_line.to_string());
                        },
                    ) {
                        Ok((true, output)) => {
                            let existed = output.contains("has already been downloaded")
                                || output.contains("[download] Skipping")
                                || output.contains("has already been recorded in the archive");

                            let message = match on_duplicate {
                                OnDuplicate::DoNothing if existed => format!(
                                    "File already exists, skipped (as per settings) in {}",
                                    yt_out_dir.display()
                                ),
                                OnDuplicate::Overwrite => {
                                    format!("Saved to {}", yt_out_dir.display())
                                }
                                OnDuplicate::CreateNew => {
                                    format!("Saved to {}", yt_out_dir.display())
                                }
                                _ => {
                                    if existed {
                                        format!(
                                            "File already exists in {}",
                                            yt_out_dir.display()
                                        )
                                    } else {
                                        format!("Saved to {}", yt_out_dir.display())
                                    }
                                }
                            };

                            emit_status(&window, true, message);
                            // println!("[tauri] yt-dlp ok with {browser}");

                            if let Ok(db) = crate::database::Database::new() {
                                let mut files = parse_multiple_filenames_from_output(
                                    &output,
                                    &processed_url,
                                    Some(&yt_out_dir),
                                );

                                // Force DB name to IG id for reels/posts
                                if processed_url.contains("instagram.com/") {
                                    if let (_, Some(id)) = ig_handle_and_id(&processed_url) {
                                        for f in files.iter_mut() {
                                            f.1 = id.clone();
                                        }
                                    }
                                }

                                let image_set_id = if files.len() > 1 {
                                    Some(uuid::Uuid::new_v4().to_string())
                                } else {
                                    None
                                };

                                for (mut user_handle, clean_name, mut file_path) in files {
                                    if processed_url.contains("instagram.com/")
                                        && user_handle == "Unknown"
                                    {
                                        if let (Some(h), _) = ig_handle_and_id(&processed_url) {
                                            user_handle = h;
                                        }
                                    }

                                    if file_path.is_empty() {
                                        file_path = "unknown_path".to_string();
                                    }

                                    let download = crate::database::Download {
                                        id: None,
                                        platform: if processed_url.contains("youtube.com")
                                            || processed_url.contains("youtu.be")
                                        {
                                            crate::database::Platform::Youtube
                                        } else if processed_url.contains("instagram.com") {
                                            crate::database::Platform::Instagram
                                        } else if processed_url.contains("tiktok.com") {
                                            crate::database::Platform::Tiktok
                                        } else {
                                            crate::database::Platform::Youtube
                                        },
                                        name: clean_name,
                                        media: if is_ig_images || is_tt_photo {
                                            crate::database::MediaKind::Image
                                        } else {
                                            crate::database::MediaKind::Video
                                        },
                                        user: user_handle,
                                        origin: crate::database::Origin::Manual,
                                        link: processed_url.clone(),
                                        status: crate::database::DownloadStatus::Done,
                                        path: file_path,
                                        image_set_id: image_set_id.clone(),
                                        date_added: Utc::now(),
                                        date_downloaded: Some(Utc::now()),
                                    };
                                    let _ = db.insert_download(&download);
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
pub async fn cancel_download(state: State<'_, crate::DownloadState>) -> Result<(), String> {
    println!("[BACKEND][DOWNLOADER] cancel_download called");
    if let Some(handle) = state.0.lock().unwrap().take() {
        handle.abort();
        println!("[tauri] Download cancelled.");
    }
    Ok(())
}
