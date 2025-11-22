/// Import a CSV (as text) and add all rows into the DB with status=Backlog.
/// This is the core function that processes CSV files imported via "Import list" button or drag-and-drop.
/// Expected CSV header format: Platform,Type,Handle,Media,link
/// - Platform: "instagram", "tiktok", or "youtube"
/// - Type: content source like "recommendation", "playlist", "profile", "bookmarks", etc.
/// - Handle: username or channel name
/// - Media: "Pictures" or "Video"
/// - link: the URL to download from
///
/// All imported items are stored in the database with status "Backlog" for later downloading.
/// Returns the number of successfully imported rows.

#[tauri::command]
pub async fn import_csv_to_db(
    csv_text: Option<String>,
    csvText: Option<String>,
) -> Result<u64, String> {
    // Accept both snake_case and camelCase keys from JS.
    let csv_text = csv_text
        .or(csvText)
        .ok_or_else(|| "missing argument: csv_text/csvText".to_string())?;

    import_csv_text(csv_text).await
}

pub async fn import_csv_text(csv_text: String) -> Result<u64, String> {
    println!("[BACKEND] [commands/import.rs] [import_csv_to_db]");

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .has_headers(true)
        .from_reader(csv_text.as_bytes());

    // Initialize database connection
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mut inserted: u64 = 0;

    // Process each row
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;

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
        let media = crate::database::MediaKind::from(media_s);

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
            super::parse::tiktok_id_from_url(&link)
                .or_else(|| super::parse::last_segment(&link))
                .unwrap_or_else(|| "Unknown".into())
        } else if link.contains("youtube.com/") || link.contains("youtu.be/") {
            super::parse::youtube_id_from_url(&link)
                .or_else(|| super::parse::last_segment(&link))
                .unwrap_or_else(|| "Unknown".into())
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

        let download = crate::database::Download {
            id: None,
            platform,
            name,
            media,
            user: handle,
            origin,
            link,
            output_format: crate::database::OutputFormat::Default,
            status: crate::database::DownloadStatus::Backlog,
            path: String::new(),
            image_set_id: None,
            date_added: chrono::Utc::now(),
            date_downloaded: None,
        };

        if db.insert_download(&download).is_ok() {
            inserted += 1;
        }
    }

    Ok(inserted)
}
