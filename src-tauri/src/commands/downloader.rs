use super::event::emit_status;
use super::parse::parse_multiple_filenames_from_output;
use std::{fs as std_fs, fs, io, path::{Path, PathBuf}};
use tauri::{Manager, State};
use walkdir::WalkDir;

fn ensure_parent_dir(p: &Path) {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
}

/// Returns (Some(final_path), "Created new"/"Overwrote") or (None, "Skipped")
fn move_with_policy(src: &Path, dest_dir: &Path, file_name: &str, on_duplicate: &crate::database::OnDuplicate) -> io::Result<(Option<String>, &'static str)> {
    // split name
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s.to_string(), e.to_string()),
        _ => (file_name.to_string(), String::from("bin")),
    };

    let mut target = dest_dir.join(format!("{stem}.{ext}"));

    match on_duplicate {
        crate::database::OnDuplicate::Overwrite => {
            ensure_parent_dir(&target);
            if target.exists() {
                fs::remove_file(&target).ok();
                fs::rename(src, &target)?;
                Ok((Some(target.display().to_string()), "Overwrote"))
            } else {
                fs::rename(src, &target)?;
                Ok((Some(target.display().to_string()), "Created new"))
            }
        }
        crate::database::OnDuplicate::DoNothing => {
            if target.exists() {
                // drop the temp file quietly
                let _ = fs::remove_file(src);
                Ok((None, "Skipped"))
            } else {
                ensure_parent_dir(&target);
                fs::rename(src, &target)?;
                Ok((Some(target.display().to_string()), "Created new"))
            }
        }
        crate::database::OnDuplicate::CreateNew => {
            if target.exists() {
                let mut n = 1usize;
                loop {
                    let cand = dest_dir.join(format!("{stem} ({n}).{ext}"));
                    if !cand.exists() {
                        target = cand;
                        break;
                    }
                    n += 1;
                }
            }
            ensure_parent_dir(&target);
            fs::rename(src, &target)?;
            Ok((Some(target.display().to_string()), "Created new"))
        }
    }
}

/// Move every file from `tmp` into `dest_dir` with `on_duplicate`.
/// Returns (moved_any, final_paths).
fn move_tmp_into_site_dir(tmp: &Path, dest_dir: &Path, on_duplicate: &crate::database::OnDuplicate, mut notify: impl FnMut(String)) -> io::Result<(bool, Vec<String>)> {
    let mut moved_any = false;
    let mut finals = Vec::new();

    fs::create_dir_all(dest_dir).ok();

    for entry in WalkDir::new(tmp).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let src = entry.path();

        // Flatten tmp structure: keep just the filename.
        let file_name = src.file_name().and_then(|s| s.to_str()).unwrap_or("image.bin");
        match move_with_policy(src, dest_dir, file_name, on_duplicate) {
            Ok((Some(fp), action)) => {
                moved_any = true;
                notify(format!("{action}: {fp}"));
                finals.push(fp);
            }
            Ok((None, _)) => {
                notify(format!("Skipped (exists): {}", dest_dir.join(file_name).display()));
            }
            Err(e) => {
                notify(format!("Failed to move {} → {}: {e}", src.display(), dest_dir.join(file_name).display()));
            }
        }
    }

    Ok((moved_any, finals))
}

#[tauri::command]
pub async fn download_url(
    app: tauri::AppHandle,
    url: String,
    state: State<'_, crate::DownloadState>,
    output_format: Option<String>,
    outputFormat: Option<String>,
    flat_destination: Option<bool>,
    flatDestination: Option<bool>,
) -> Result<(), String> {
    println!("[BACKEND][DOWNLOADER] download_url called with: {}", url);
    println!(
        "[DOWNLOADER] args: output_format={:?} outputFormat={:?} flat_destination={:?} flatDestination={:?}",
        output_format, outputFormat, flat_destination, flatDestination
    );
    if let Some(window) = app.get_webview_window("main") {
        // Normalize minimally: strip IG query params
        let mut processed_url = url.clone();
        if processed_url.contains("instagram.com/") {
            if let Some((base, _)) = processed_url.split_once('?') {
                processed_url = base.to_string();
            }
        }

        // Intentionally no "Starting download ..." emit — keep UI concise.

        let state_clone = state.inner().clone();
        let handle = tokio::spawn({
            let window = window.clone();
            let processed_url = processed_url.clone();
            let original_url_arg = url.clone();

            async move {
                let effective_format = output_format.or(outputFormat);
                let mut want_audio_only = matches!(effective_format.as_deref(), Some("audio"));
                // Load settings
                let s = crate::settings::load_settings();
                let download_root = PathBuf::from(s.download_directory.clone());
                let on_duplicate = s.on_duplicate.clone();

                println!("[DOWNLOADER] settings: root={} policy={:?}", download_root.display(),on_duplicate);

                if let Err(e) = std_fs::create_dir_all(&download_root) {
                    emit_status(&window, false, format!("Failed to create download dir: {e}"));
                    *state_clone.0.lock().unwrap() = None;
                    return;
                }

                // Normalize special Home flags embedded in URL (fallback when args missing)
                let has_audio_flag = processed_url.contains("#__audio_only__");
                if has_audio_flag { want_audio_only = true; }
                // If no explicit format provided and no URL flag, honor the Settings default
                if !want_audio_only && effective_format.is_none() {
                    if let crate::database::DefaultOutput::Audio = s.default_output { want_audio_only = true; }
                }
                let has_flat_flag = processed_url.contains("#__flat__");
                let cleaned_url = processed_url.replace("#__audio_only__", "").replace("#__flat__", "");

                // Finally, if still not explicitly set, consult DB row preference for this link
                if !want_audio_only && effective_format.is_none() && !has_audio_flag {
                    if let Ok(db) = crate::database::Database::new() {
                        if let Ok(fmt) = db.output_format_for_link(&cleaned_url) {
                            if fmt.eq_ignore_ascii_case("audio") { want_audio_only = true; }
                        }
                    }
                }

                // Use cleaned URL for site detection and DB matching
                let site = if cleaned_url.contains("instagram.com") {
                    "instagram"
                } else if cleaned_url.contains("tiktok.com") {
                    "tiktok"
                } else if cleaned_url.contains("youtube.com") || cleaned_url.contains("youtu.be")
                {
                    "youtube"
                } else if cleaned_url.contains("pinterest.com") || cleaned_url.contains("pin.it") {
                    "pinterest"
                } else {
                    "other"
                };

                // Determine collection folder (origin - user_handle) under the platform
                let collection_dir_label = match crate::database::Database::new()
                    .ok()
                    .and_then(|db| db.collection_for_link(&cleaned_url).ok().flatten())
                {
                    Some(info) => crate::database::Database::collection_folder_label(&info.origin, &info.user_handle),
                    None => crate::database::Database::collection_folder_label("manual", "Unknown"),
                };

                // Final destination: flat when requested (home page), else structured
                let use_flat = flat_destination.or(flatDestination).unwrap_or(false) || has_flat_flag;
                let dest_dir = if use_flat {
                    download_root.clone()
                } else {
                    download_root.join(site).join(collection_dir_label)
                };
                println!("[DOWNLOADER] flat_destination={} dest_dir={} audio_only={}", use_flat, dest_dir.display(), want_audio_only);
                let _ = std_fs::create_dir_all(&dest_dir);

                let is_instagram = cleaned_url.contains("instagram.com/");
                let is_ig_post_p = is_instagram && cleaned_url.contains("/p/");
                let is_tt_photo = crate::utils::url::is_tiktok_photo(&cleaned_url);

                let browsers = crate::utils::os::installed_browsers();

                'browser_loop: for (browser, cookie_arg) in &browsers {
                    println!("[DOWNLOADER] trying with cookies from {browser}; dest={}", dest_dir.display());

                    // ─── Instagram: try yt-dlp first; if /p/ fails, fallback to gallery-dl ───
                    if is_instagram {
                        let window_clone = window.clone();
                        let effective_url = if want_audio_only { format!("{}#__audio_only__", cleaned_url) } else { cleaned_url.clone() };
                        match crate::download::video::run_yt_dlp_with_progress(
                            &dest_dir,
                            cookie_arg,
                            &effective_url,
                            /* is_ig_images = */ false,
                            &on_duplicate,
                            |progress_line| {
                                emit_status(&window_clone, false, progress_line.to_string());
                            },
                        ) {
                            Ok((true, output)) => {
                                let status_msg: String = if want_audio_only { "Saved (audio)".to_string() } else { "Saved (video)".to_string() };
                                emit_status(&window, true, status_msg);
                                if let Ok(db) = crate::database::Database::new() {
                                    let files = parse_multiple_filenames_from_output(&output, &cleaned_url, Some(&dest_dir));
                                    let first_path = files.get(0).map(|t| t.2.clone()).unwrap_or_default();
                                    let _ = db.mark_link_done(&cleaned_url, &first_path);
                                }

                                *state_clone.0.lock().unwrap() = None;
                                return;
                            }
                            // if yt-dlp failed AND it's an IG /p/ (images), fallback to gallery-dl
                            Ok((false, _)) | Err(_) => {
                                if is_ig_post_p {
                                    println!("[DOWNLOADER][IMAGES] IG /p/ fallback via gallery-dl; policy={:?}", on_duplicate);
                                    match crate::download::image::run_gallery_dl_to_temp(&download_root, &processed_url, cookie_arg) {
                                        Ok((output, tmp_dir)) if output.status.success() => {
                                            // Move with policy
                                            let (moved_any, finals) = move_tmp_into_site_dir(
                                                &tmp_dir,
                                                &dest_dir,
                                                &on_duplicate,
                                                |line| {
                                                    println!("[DOWNLOADER][IMAGES] {line}");
                                                    emit_status(&window, true, line);
                                                },
                                            )
                                            .unwrap_or((false, vec![]));

                                            let _ = fs::remove_dir_all(&tmp_dir);

                                            if moved_any {
                                                if let Ok(db) = crate::database::Database::new() {
                                                    let first = finals.get(0).cloned().unwrap_or_default();
                                                    let _ = db.mark_link_done(&original_url_arg, &first);
                                                }

                                                emit_status(&window, true, format!("Saved images"));
                                                *state_clone.0.lock().unwrap() = None;
                                                return;
                                            } else {
                                                eprintln!("[DOWNLOADER][IMAGES] No files moved from {} -> {}", tmp_dir.display(), dest_dir.display());
                                            }
                                        }
                                        Ok((output, tmp_dir)) => {
                                            eprintln!("[tauri] gallery-dl failed (IG fallback) with {browser}; tmp={}\nstderr:\n{}", tmp_dir.display(), String::from_utf8_lossy(&output.stderr));
                                            let _ = fs::remove_dir_all(&tmp_dir);
                                        }
                                        Err(e) => {
                                            eprintln!("[tauri] gallery-dl error (IG fallback) with {browser}: {e}");
                                        }
                                    }
                                }
                                // Otherwise: try next browser
                            }
                        }

                        continue 'browser_loop;
                    }

                    // ─── Pinterest and TikTok photo → gallery-dl (to temp) ───
                    if site == "pinterest" {
                        println!(
                            "[DOWNLOADER][IMAGES] Pinterest via gallery-dl; policy={:?}",
                            on_duplicate
                        );
                        match crate::download::image::run_gallery_dl_to_temp(&download_root, &cleaned_url, cookie_arg) {
                            Ok((output, tmp_dir)) if output.status.success() => {
                                let (moved_any, finals) = move_tmp_into_site_dir(
                                    &tmp_dir,
                                    &dest_dir,
                                    &on_duplicate,
                                    |line| {
                                        println!("[DOWNLOADER][IMAGES] {line}");
                                        emit_status(&window, true, line);
                                    },
                                )
                                .unwrap_or((false, vec![]));

                                let _ = fs::remove_dir_all(&tmp_dir);

                                if moved_any {
                                    if let Ok(db) = crate::database::Database::new() {
                                        let first = finals.get(0).cloned().unwrap_or_default();
                                        let _ = db.mark_link_done(&cleaned_url, &first);
                                    }

                                    emit_status(&window, true, format!("Saved images"));
                                    *state_clone.0.lock().unwrap() = None;
                                    return;
                                } else {
                                    eprintln!("[DOWNLOADER][IMAGES] No files moved from {} -> {}", tmp_dir.display(), dest_dir.display());
                                }
                            }
                            Ok((output, tmp_dir)) => {
                                eprintln!("[tauri] gallery-dl failed (Pinterest) with {browser}; tmp={}\nstderr:\n{}", tmp_dir.display(), String::from_utf8_lossy(&output.stderr));
                                let _ = fs::remove_dir_all(&tmp_dir);
                            }
                            Err(e) => {
                                eprintln!("[tauri] gallery-dl error (Pinterest) with {browser}: {e}");
                            }
                        }

                        continue 'browser_loop;
                    }

                    // ─── TikTok photo → gallery-dl (to temp) ───
                    if is_tt_photo {
                        println!(
                            "[DOWNLOADER][IMAGES] TikTok photo via gallery-dl; policy={:?}",
                            on_duplicate
                        );
                        match crate::download::image::run_gallery_dl_to_temp(&download_root, &cleaned_url, cookie_arg) {
                            Ok((output, tmp_dir)) if output.status.success() => {
                                let (moved_any, finals) = move_tmp_into_site_dir(
                                    &tmp_dir,
                                    &dest_dir,
                                    &on_duplicate,
                                    |line| {
                                        println!("[DOWNLOADER][IMAGES] {line}");
                                        emit_status(&window, true, line);
                                    },
                                )
                                .unwrap_or((false, vec![]));

                                let _ = fs::remove_dir_all(&tmp_dir);

                                if moved_any {
                                    if let Ok(db) = crate::database::Database::new() {
                                        let first = finals.get(0).cloned().unwrap_or_default();
                                        let _ = db.mark_link_done(&cleaned_url, &first);
                                    }

                                    emit_status(&window, true, format!("Saved images"));
                                    *state_clone.0.lock().unwrap() = None;
                                    return;
                                } else {
                                    eprintln!("[DOWNLOADER][IMAGES] No files moved from {} -> {}", tmp_dir.display(), dest_dir.display());
                                }
                            }
                            Ok((output, tmp_dir)) => {
                                eprintln!("[tauri] gallery-dl failed (TT photo) with {browser}; tmp={}\nstderr:\n{}", tmp_dir.display(), String::from_utf8_lossy(&output.stderr));
                                let _ = fs::remove_dir_all(&tmp_dir);
                            }
                            Err(e) => {
                                eprintln!("[tauri] gallery-dl error (TT photo) with {browser}: {e}");
                            }
                        }

                        continue 'browser_loop;
                    }

                    // ─── Generic yt-dlp (YouTube, TikTok video, etc.) ───
                    let window_clone = window.clone();
                    let effective_url = if want_audio_only { format!("{}#__audio_only__", cleaned_url) } else { cleaned_url.clone() };
                    match crate::download::video::run_yt_dlp_with_progress(
                        &dest_dir,
                        cookie_arg,
                        &effective_url,
                        /* is_ig_images = */ false,
                        &on_duplicate,
                        |progress_line| {
                            emit_status(&window_clone, false, progress_line.to_string());
                        },
                    ) {
                        Ok((true, output)) => {
                            let status_msg: String = if want_audio_only { "Saved (audio)".to_string() } else { "Saved (video)".to_string() };
                            emit_status(&window, true, status_msg);
                            if let Ok(db) = crate::database::Database::new() {
                                let files = parse_multiple_filenames_from_output(&output, &cleaned_url, Some(&dest_dir));
                                let first_path = files.get(0).map(|t| t.2.clone()).unwrap_or_default();
                                let _ = db.mark_link_done(&cleaned_url, &first_path);
                            }

                            *state_clone.0.lock().unwrap() = None;
                            return;
                        }
                        Ok((false, output)) => {
                            eprintln!("[tauri] yt-dlp failed with browser: {browser}\noutput:\n{}", output);
                        }
                        Err(e) => {
                            eprintln!("[tauri] Failed to exec yt-dlp for {browser}: {e}");
                        }
                    }
                }

                // All browsers failed
                if is_instagram || is_tt_photo {
                    emit_status(&window, false, "Failed to fetch media. Ensure `yt-dlp`/`gallery-dl` are installed and your chosen browser is logged in.");
                } else {
                    emit_status(&window, false, "Failed to download with any available browser's cookies.");
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
