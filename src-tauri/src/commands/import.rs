// ===== src-tauri/src/commands/import.rs =====
use super::parse::{ig_handle_and_id, last_segment, tiktok_id_from_url, youtube_id_from_url};
use chrono::Utc;

/// Import a CSV (as text) and add all rows into the DB with status=Backlog.
/// Expected header: Platform,Type,Handle,Media,link
#[tauri::command]
pub async fn import_csv_to_db(csv_text: String) -> Result<u64, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .has_headers(true)
        .from_reader(csv_text.as_bytes());

    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mut inserted: u64 = 0;

    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        let platform_s = rec.get(0).unwrap_or("").to_string();
        let typ_s = rec.get(1).unwrap_or("").to_lowercase();
        let mut handle = rec.get(2).unwrap_or("Unknown").to_string();
        let media_s = rec.get(3).unwrap_or("").to_string();
        let link = rec.get(4).unwrap_or("").to_string();
        if link.is_empty() { continue; }

        let platform = crate::database::Platform::from(platform_s);
        let media = crate::database::MediaKind::from(media_s);
        let origin = match typ_s.as_str() {
            "recommendation" => crate::database::Origin::Recommendation,
            "playlist" => crate::database::Origin::Playlist,
            "profile" => crate::database::Origin::Profile,
            "bookmarks" => crate::database::Origin::Bookmarks,
            "liked" | "reposts" => crate::database::Origin::Other,
            _ => crate::database::Origin::Other,
        };

        let name = if link.contains("instagram.com/") {
            if let (_, Some(id)) = ig_handle_and_id(&link) { id }
            else { last_segment(&link).unwrap_or_else(|| "Unknown".into()) }
        } else if link.contains("tiktok.com/") {
            tiktok_id_from_url(&link).or_else(|| last_segment(&link)).unwrap_or_else(|| "Unknown".into())
        } else if link.contains("youtube.com/") || link.contains("youtu.be/") {
            youtube_id_from_url(&link).or_else(|| last_segment(&link)).unwrap_or_else(|| "Unknown".into())
        } else {
            last_segment(&link).unwrap_or_else(|| "Unknown".into())
        };

        if (handle.is_empty() || handle == "Unknown") && link.contains("instagram.com/") {
            if let (Some(h), _) = ig_handle_and_id(&link) { handle = h; }
        }

        let download = crate::database::Download {
            id: None,
            platform,
            name,
            media,
            user: handle,
            origin,
            link,
            status: crate::database::DownloadStatus::Backlog,
            path: String::new(),
            image_set_id: None,
            date_added: Utc::now(),
            date_downloaded: None,
        };

        if db.insert_download(&download).is_ok() { inserted += 1; }
    }

    Ok(inserted)
}
