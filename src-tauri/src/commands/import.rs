// ===== src-tauri/src/commands/import.rs =====
use super::parse::{ig_handle_and_id, last_segment, tiktok_id_from_url, youtube_id_from_url};
use chrono::Utc;

/// [BACKEND] [commands/import.rs] [import_csv_to_db]
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
pub async fn import_csv_to_db(csv_text: String) -> Result<u64, String> {
    println!("[BACKEND] [commands/import.rs] [import_csv_to_db]");
    println!("Importing CSV to DB: {}", csv_text);
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .has_headers(true)
        .from_reader(csv_text.as_bytes());

    // Initialize database connection
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mut inserted: u64 = 0;

    // Process each row in the CSV file
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        // Extract CSV columns (Platform,Type,Handle,Media,link)
        let platform_s = rec.get(0).unwrap_or("").to_string();
        let typ_s = rec.get(1).unwrap_or("").to_lowercase();
        let mut handle = rec.get(2).unwrap_or("Unknown").to_string();
        let media_s = rec.get(3).unwrap_or("").to_string();
        let link = rec.get(4).unwrap_or("").to_string();
        // Skip empty links
        if link.is_empty() { continue; }

        // Convert string values to database enums
        let platform = crate::database::Platform::from(platform_s);
        let media = crate::database::MediaKind::from(media_s);

        // Map CSV "Type" column to database Origin enum
        // This determines how the content was sourced (algorithm recommendation, playlist, etc.)
        let origin = match typ_s.as_str() {
            "recommendation" => crate::database::Origin::Recommendation,
            "playlist" => crate::database::Origin::Playlist,
            "profile" => crate::database::Origin::Profile,
            "bookmarks" => crate::database::Origin::Bookmarks,
            "liked" | "reposts" => crate::database::Origin::Other,
            _ => crate::database::Origin::Other,
        };

        // Extract content name/ID from URL based on platform
        // This is used as the filename when downloading
        let name = if link.contains("instagram.com/") {
            // For Instagram, try to extract post/reel ID, fallback to last URL segment
            if let (_, Some(id)) = ig_handle_and_id(&link) { id }
            else { last_segment(&link).unwrap_or_else(|| "Unknown".into()) }
        } else if link.contains("tiktok.com/") {
            // For TikTok, extract video ID, fallback to last URL segment
            tiktok_id_from_url(&link).or_else(|| last_segment(&link)).unwrap_or_else(|| "Unknown".into())
        } else if link.contains("youtube.com/") || link.contains("youtu.be/") {
            // For YouTube, extract video ID, fallback to last URL segment
            youtube_id_from_url(&link).or_else(|| last_segment(&link)).unwrap_or_else(|| "Unknown".into())
        } else {
            // For other platforms, use last URL segment as name
            last_segment(&link).unwrap_or_else(|| "Unknown".into())
        };

        // Extract username/handle from Instagram URLs if not provided in CSV
        // This ensures we have proper user attribution for Instagram content
        if (handle.is_empty() || handle == "Unknown") && link.contains("instagram.com/") {
            if let (Some(h), _) = ig_handle_and_id(&link) { handle = h; }
        }

        // Create database record for this CSV row
        // Status is set to "Backlog" so it will be available for downloading later
        // Path is empty since file hasn't been downloaded yet
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

        // Insert into database and count successful insertions
        if db.insert_download(&download).is_ok() { inserted += 1; }
    }

    Ok(inserted)
}
