use crate::utils::resource::{classify_url, Resource};

/// Import downloadable items and browser-only references from a CSV.
/// This is the core function that processes CSV files imported via "Import list" button or drag-and-drop.
/// Expected CSV header format: Platform,Type,Handle,Media,link
/// - Platform: "patreon", "pinterest", "instagram", "tiktok", or "youtube"
/// - Type: content source like "recommendation", "playlist", "profile", "bookmarks", etc.
/// - Handle: username or channel name
/// - Media: "pictures", "video", or "link"
/// - link: the URL to download from
///
/// Downloadable items enter the backlog. External references are visible only in Links.
/// Returns the number of successfully imported rows.

#[tauri::command]
pub async fn import_csv_to_db(app: tauri::AppHandle, csv_text: Option<String>, csvText: Option<String>) -> Result<u64, String> {
    // Accept both snake_case and camelCase keys from JS.
    let csv_text = csv_text.or(csvText).ok_or_else(|| "missing argument: csv_text/csvText".to_string())?;

    let inserted = import_csv_text(csv_text).await?;
    use tauri::Emitter;
    let _ = app.emit("import_completed", inserted);
    Ok(inserted)
}

pub async fn import_csv_text(csv_text: String) -> Result<u64, String> {
    // CSV parsing + rusqlite are blocking; keep them off the async runtime.
    tauri::async_runtime::spawn_blocking(move || import_csv_text_blocking(csv_text)).await.map_err(|e| format!("Join error: {e}"))?
}

fn import_csv_text_blocking(csv_text: String) -> Result<u64, String> {
    println!("[BACKEND] [commands/import.rs] [import_csv_to_db]");

    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    import_into(&db, &csv_text)
}

pub(crate) fn import_into(db: &crate::database::Database, csv_text: &str) -> Result<u64, String> {
    let mut rdr = csv::ReaderBuilder::new().trim(csv::Trim::All).has_headers(true).from_reader(csv_text.as_bytes());

    // Batch every insert into one transaction: N implicit transactions (one
    // fsync each) collapse into a single commit. The duplicate check reads the
    // same connection, so rows inserted earlier in the loop are still visible.
    db.with_transaction(|| {
        let mut inserted: u64 = 0;

        // Process each row
        for rec in rdr.records() {
            let rec = match rec {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[import] skipping malformed CSV row: {e}");
                    continue;
                }
            };

            // Extract CSV columns (Platform,Type,Handle,Media,link)
            let platform_s = rec.get(0).unwrap_or("").to_string();
            let typ_s = rec.get(1).unwrap_or("").to_lowercase();
            let mut handle = rec.get(2).unwrap_or("Unknown").to_string();
            let media_s = rec.get(3).unwrap_or("").to_string();
            let link = rec.get(4).unwrap_or("").to_string();
            if link.is_empty() {
                continue;
            }

            let platform = crate::database::Platform::from(platform_s.clone());
            let parsed = crate::utils::url::http_url(&link)?;
            let route = classify_url(&parsed, &media_s);
            let media = route.media();
            let status = if route == Resource::Reference {
                crate::database::DownloadStatus::Reference
            } else {
                crate::database::DownloadStatus::Backlog
            };

            // Determine origin; special-case Pinterest "{user} - {something}" to pinboard
            let origin = if platform_s.eq_ignore_ascii_case("pinterest") {
                let is_pinboard = handle.contains(" - ");
                if is_pinboard {
                    crate::database::Origin::Pinboard
                } else {
                    crate::database::Origin::Profile
                }
            } else {
                match typ_s.as_str() {
                    "recommendation" => crate::database::Origin::Recommendation,
                    "collection" => crate::database::Origin::Collection,
                    "playlist" => crate::database::Origin::Playlist,
                    "profile" => crate::database::Origin::Profile,
                    "bookmarks" => crate::database::Origin::Bookmarks,
                    "liked" | "reposts" => crate::database::Origin::Other,
                    _ => crate::database::Origin::Other,
                }
            };

            // Derive a sensible name from the URL per platform
            let name = if link.contains("instagram.com/") {
                if let (_, Some(id)) = super::parse::ig_handle_and_id(&link) {
                    id
                } else {
                    super::parse::last_segment(&link).unwrap_or_else(|| "Unknown".into())
                }
            } else if link.contains("tiktok.com/") {
                super::parse::tiktok_id_from_url(&link).or_else(|| super::parse::last_segment(&link)).unwrap_or_else(|| "Unknown".into())
            } else if link.contains("youtube.com/") || link.contains("youtu.be/") {
                super::parse::youtube_id_from_url(&link).or_else(|| super::parse::last_segment(&link)).unwrap_or_else(|| "Unknown".into())
            } else if link.contains("pinterest.com/") || link.contains("pin.it/") {
                super::parse::last_segment(&link).unwrap_or_else(|| "Unknown".into())
            } else {
                super::parse::last_segment(&link).unwrap_or_else(|| "Unknown".into())
            };

            // Fill in IG handle if missing
            if (handle.is_empty() || handle == "Unknown") && link.contains("instagram.com/") {
                if let (Some(h), _) = super::parse::ig_handle_and_id(&link) {
                    handle = h;
                }
            }

            // Normalize empty handles to "Unknown" so the UI queue callback
            // (which displays "Unknown") matches the stored value.
            if handle.trim().is_empty() {
                handle = "Unknown".into();
            }

            if platform == crate::database::Platform::Patreon && origin == crate::database::Origin::Collection {
                handle = db.patreon_import_handle(&handle, &parsed).map_err(|e| e.to_string())?;
            }
            let platform_token = format!("{:?}", platform).to_lowercase();
            let origin_token = format!("{:?}", origin).to_lowercase();
            if let Some((id, status, path)) = db.existing_collection_item(&link, &platform_token, &handle, &origin_token, matches!(media, crate::database::MediaKind::Link | crate::database::MediaKind::File) || parsed.query().is_some() || parsed.fragment().is_some()).map_err(|e| e.to_string())? {
                let old_shortcut = status == "done" && path.to_ascii_lowercase().ends_with(".url");
                let stronger_hint = status == "reference" && route != classify_url(&parsed, "");
                if route != Resource::Reference && (old_shortcut || stronger_hint) {
                    inserted += db.restore_media_download(id, &media).map_err(|e| e.to_string())? as u64;
                }
                continue;
            }

            let download = crate::database::Download {id: None, platform, name, media, user: handle, origin, link, output_format: crate::database::OutputFormat::Default, status, path: String::new(), image_set_id: None, last_error: None, date_added: chrono::Utc::now(), date_downloaded: None};

            db.insert_download(&download).map_err(|e| e.to_string())?;
            inserted += 1;
        }

        Ok(inserted)
    })
}
