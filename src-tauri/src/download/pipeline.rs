use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::commands::parse::parse_multiple_filenames_from_output;
use crate::database::DbDownloadRow;
use crate::database::{Database, DefaultOutput, OnDuplicate};
use crate::download::image;
use crate::download::manager::{DownloadEvent, DownloadOverrides};
use crate::download::video;

use crate::settings;
use crate::utils;

use tauri::AppHandle;

use walkdir::WalkDir;

fn ensure_parent_dir(p: &Path) {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
}

fn move_with_policy(
    src: &Path,
    dest_dir: &Path,
    file_name: &str,
    on_duplicate: &OnDuplicate,
) -> std::io::Result<(Option<String>, &'static str)> {
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s.to_string(), e.to_string()),
        _ => (file_name.to_string(), String::from("bin")),
    };
    let mut target = dest_dir.join(format!("{stem}.{ext}"));
    match on_duplicate {
        OnDuplicate::Overwrite => {
            ensure_parent_dir(&target);
            if target.exists() {
                fs::remove_file(&target).ok();
            }
            fs::copy(src, &target)?;
            fs::remove_file(src)?;
            Ok((Some(target.display().to_string()), "Overwrote"))
        }
        OnDuplicate::DoNothing => {
            if target.exists() {
                let _ = fs::remove_file(src);
                Ok((None, "Skipped"))
            } else {
                ensure_parent_dir(&target);
                fs::copy(src, &target)?;
                fs::remove_file(src)?;
                Ok((Some(target.display().to_string()), "Created new"))
            }
        }
        OnDuplicate::CreateNew => {
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
            fs::copy(src, &target)?;
            fs::remove_file(src)?;
            Ok((Some(target.display().to_string()), "Created new"))
        }
    }
}

fn move_tmp_into_site_dir(
    tmp: &Path,
    dest_dir: &Path,
    on_duplicate: &OnDuplicate,
    mut notify: impl FnMut(String),
) -> std::io::Result<(bool, Vec<String>)> {
    let mut moved_any = false;
    let mut finals = Vec::new();
    fs::create_dir_all(dest_dir).ok();

    for entry in WalkDir::new(tmp).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let src = entry.path();
        let file_name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image.bin");
        match move_with_policy(src, dest_dir, file_name, on_duplicate) {
            Ok((Some(fp), action)) => {
                moved_any = true;
                notify(format!("{action}: {fp}"));
                finals.push(fp);
            }
            Ok((None, _)) => notify(format!(
                "Skipped (exists): {}",
                dest_dir.join(file_name).display()
            )),
            Err(e) => notify(format!(
                "Failed to move {} → {}: {e}",
                src.display(),
                dest_dir.join(file_name).display()
            )),
        }
    }
    Ok((moved_any, finals))
}

pub async fn execute_download_job(
    app: AppHandle,
    row: DbDownloadRow,
    overrides: Option<DownloadOverrides>,
    emitter: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
) -> Result<Option<String>, String> {
    let settings = settings::load_settings();
    let download_root = PathBuf::from(settings.download_directory.clone());
    if let Err(e) = fs::create_dir_all(&download_root) {
        return Err(format!("Failed to create download dir: {e}"));
    }

    let mut want_audio_pref = overrides.as_ref().and_then(|ov| ov.force_audio);
    let (mut cleaned_url, legacy_audio_flag, legacy_flat_flag) = strip_legacy_flags(&row.link);
    if want_audio_pref.is_none() {
        want_audio_pref = match row.output_format.to_lowercase().as_str() {
            "audio" => Some(true),
            "video" => Some(false),
            _ => None,
        };
    }
    if want_audio_pref.is_none() {
        want_audio_pref = Some(matches!(settings.default_output, DefaultOutput::Audio));
    }
    if legacy_audio_flag {
        want_audio_pref = Some(true);
    }
    let want_audio_only = want_audio_pref.unwrap_or(false);

    let mut use_flat = overrides
        .as_ref()
        .map(|ov| ov.flat_destination)
        .unwrap_or(false);
    if legacy_flat_flag {
        use_flat = true;
    }

    if cleaned_url.contains("instagram.com/") {
        if let Some((base, _)) = cleaned_url.split_once('?') {
            cleaned_url = base.to_string();
        }
    }

    let site = infer_site(&cleaned_url);
    let collection_dir_label = Database::collection_folder_label(&row.origin, &row.user_handle);
    let dest_dir = if use_flat {
        download_root.clone()
    } else {
        download_root.join(site).join(collection_dir_label)
    };
    let _ = fs::create_dir_all(&dest_dir);

    let is_instagram = cleaned_url.contains("instagram.com/");
    let is_ig_post_p = is_instagram && cleaned_url.contains("/p/");
    let is_tt_photo = utils::url::is_tiktok_photo(&cleaned_url);

    let browsers = utils::os::installed_browsers();
    if browsers.is_empty() {
        return Err("No logged-in browsers detected for cookies.".into());
    }

    let mut last_error: Option<String> = None;
    for (browser, cookie_arg) in &browsers {
        (emitter)(DownloadEvent::Message {
            id: row.id,
            message: format!("Trying {} cookies; dest={}", browser, dest_dir.display()),
        });

        if is_instagram {
            let effective_url = if want_audio_only {
                format!("{}#__audio_only__", cleaned_url)
            } else {
                cleaned_url.clone()
            };
            match video::run_yt_dlp_with_progress(
                &app,
                &dest_dir,
                cookie_arg,
                &effective_url,
                false,
                &settings.on_duplicate,
                row.id,
                emitter.clone(),
            )
            .await
            {
                Ok((true, output)) => {
                    (emitter)(DownloadEvent::Message {
                        id: row.id,
                        message: if want_audio_only {
                            "Saved (audio)".into()
                        } else {
                            "Saved (video)".into()
                        },
                    });
                    let files = parse_multiple_filenames_from_output(
                        &output,
                        &cleaned_url,
                        Some(&dest_dir),
                    );
                    return Ok(files.get(0).map(|t| t.2.clone()));
                }
                Ok((false, _)) | Err(_) => {
                    if is_ig_post_p {
                        match image::run_gallery_dl_to_temp(
                            &app,
                            &download_root,
                            &cleaned_url,
                            cookie_arg,
                            row.id,
                            emitter.clone(),
                        )
                        .await
                        {
                            Ok((ok, _out, tmp_dir)) if ok => {
                                let (moved_any, finals) = move_tmp_into_site_dir(
                                    &tmp_dir,
                                    &dest_dir,
                                    &settings.on_duplicate,
                                    |line| {
                                        (emitter)(DownloadEvent::Message {
                                            id: row.id,
                                            message: line,
                                        });
                                    },
                                )
                                .unwrap_or((false, vec![]));
                                let _ = fs::remove_dir_all(&tmp_dir);
                                if moved_any {
                                    (emitter)(DownloadEvent::Message {
                                        id: row.id,
                                        message: "Saved images".into(),
                                    });
                                    return Ok(finals.get(0).cloned());
                                } else {
                                    last_error =
                                        Some(format!("No files moved from {}", tmp_dir.display()));
                                }
                            }
                            Ok((_ok, output, tmp_dir)) => {
                                let msg = format!(
                                    "gallery-dl failed (IG fallback) tmp={}\n{}",
                                    tmp_dir.display(),
                                    output
                                );
                                last_error = Some(msg.clone());
                                (emitter)(DownloadEvent::Message {
                                    id: row.id,
                                    message: msg,
                                });
                                let _ = fs::remove_dir_all(&tmp_dir);
                            }
                            Err(e) => {
                                last_error = Some(e.to_string());
                            }
                        }
                    }
                }
            }
            continue;
        }

        if site == "pinterest" || is_tt_photo {
            match image::run_gallery_dl_to_temp(
                &app,
                &download_root,
                &cleaned_url,
                cookie_arg,
                row.id,
                emitter.clone(),
            )
            .await
            {
                Ok((ok, _output, tmp_dir)) if ok => {
                    let (moved_any, finals) = move_tmp_into_site_dir(
                        &tmp_dir,
                        &dest_dir,
                        &settings.on_duplicate,
                        |line| {
                            (emitter)(DownloadEvent::Message {
                                id: row.id,
                                message: line,
                            });
                        },
                    )
                    .unwrap_or((false, vec![]));
                    let _ = fs::remove_dir_all(&tmp_dir);
                    if moved_any {
                        (emitter)(DownloadEvent::Message {
                            id: row.id,
                            message: "Saved images".into(),
                        });
                        return Ok(finals.get(0).cloned());
                    } else {
                        last_error = Some(format!("No files moved from {}", tmp_dir.display()));
                    }
                }
                Ok((_ok, output, tmp_dir)) => {
                    let msg = format!("gallery-dl failed tmp={}\n{}", tmp_dir.display(), output);
                    last_error = Some(msg.clone());
                    (emitter)(DownloadEvent::Message {
                        id: row.id,
                        message: msg,
                    });
                    let _ = fs::remove_dir_all(&tmp_dir);
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
            continue;
        }

        let effective_url = if want_audio_only {
            format!("{}#__audio_only__", cleaned_url)
        } else {
            cleaned_url.clone()
        };
        match video::run_yt_dlp_with_progress(
            &app,
            &dest_dir,
            cookie_arg,
            &effective_url,
            false,
            &settings.on_duplicate,
            row.id,
            emitter.clone(),
        )
        .await
        {
            Ok((true, output)) => {
                (emitter)(DownloadEvent::Message {
                    id: row.id,
                    message: if want_audio_only {
                        "Saved (audio)".into()
                    } else {
                        "Saved (video)".into()
                    },
                });
                let files =
                    parse_multiple_filenames_from_output(&output, &cleaned_url, Some(&dest_dir));
                return Ok(files.get(0).map(|t| t.2.clone()));
            }
            Ok((false, output)) => {
                let msg = format!("yt-dlp failed with browser: {browser}\noutput:\n{output}");
                last_error = Some(msg.clone());
                (emitter)(DownloadEvent::Message {
                    id: row.id,
                    message: msg,
                });
            }
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        if is_instagram || is_tt_photo {
            "Failed to fetch media. Ensure bundled tools are present and your browser is logged in."
                .into()
        } else {
            "Failed to download with available browser cookies.".into()
        }
    }))
}

fn strip_legacy_flags(url: &str) -> (String, bool, bool) {
    let mut cleaned = url.to_string();
    let mut want_audio = false;
    let mut flat = false;
    if cleaned.contains("#__audio_only__") {
        want_audio = true;
        cleaned = cleaned.replace("#__audio_only__", "");
    }
    if cleaned.contains("#__flat__") {
        flat = true;
        cleaned = cleaned.replace("#__flat__", "");
    }
    (cleaned, want_audio, flat)
}

fn infer_site(url: &str) -> &'static str {
    if url.contains("instagram.com") {
        "instagram"
    } else if url.contains("tiktok.com") {
        "tiktok"
    } else if url.contains("youtube.com") || url.contains("youtu.be") {
        "youtube"
    } else if url.contains("pinterest.com") || url.contains("pin.it") {
        "pinterest"
    } else {
        "other"
    }
}
