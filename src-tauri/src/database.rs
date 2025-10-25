use chrono::{DateTime, Utc};
use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
}

/* ----------------------------- enums & models ----------------------------- */
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Platform {
    Youtube,
    Tiktok,
    Instagram,
    Pinterest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Origin {
    Recommendation,
    Playlist,
    Profile,
    Bookmarks,
    Pinboard,
    Other,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    Queue,
    Backlog,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: Option<i64>,
    pub platform: Platform,
    pub name: String,
    pub media: MediaKind,
    pub user: String,
    pub origin: Origin,
    pub link: String,
    /// Desired output format for downloader/transcoder; default="default"
    pub output_format: OutputFormat,
    pub status: DownloadStatus,
    pub path: String,
    pub image_set_id: Option<String>,
    pub date_added: DateTime<Utc>,
    pub date_downloaded: Option<DateTime<Utc>>,
}

/// Row shape returned to the **frontend** for the Downloads page.
/// Keys and value tokens match `src/types.rs` expectations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiBacklogRow {
    #[serde(rename = "Platform")]           /// "instagram" | "tiktok" | "youtube"
    pub platform: String,
    #[serde(rename = "Type")]               /// "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts"
    pub content_type: String,
    #[serde(rename = "Handle")]             /// username / channel
    pub handle: String,
    #[serde(rename = "Media")]              /// "pictures" | "video"
    pub media: String,
    pub link: String,
    /// Optional output preference for the row ("audio" | "video" | "default").
    #[serde(default)]
    pub output_format: String,
}

/// Lightweight info for deciding the destination collection directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub platform: String,      // "instagram" | "tiktok" | "youtube"
    pub origin: String,        // already lowercase token
    pub user_handle: String,   // original user handle (may be "Unknown")
}

/* ------------------------------ conversions ------------------------------ */

impl From<String> for Platform {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "youtube"       => Platform::Youtube,
            "tiktok"        => Platform::Tiktok,
            "instagram"     => Platform::Instagram,
            "pinterest"     => Platform::Pinterest,
            _               => Platform::Youtube,               // Default fallback
        }
    }
}
impl From<String> for MediaKind {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "image" | "images"          => MediaKind::Image,
            "video" | "videos"          => MediaKind::Video,
            _                           => MediaKind::Video,    // Default fallback
        }
    }
}

impl From<String> for Origin {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "recommendation"    => Origin::Recommendation,
            "playlist"          => Origin::Playlist,
            "profile"           => Origin::Profile,
            "bookmarks"         => Origin::Bookmarks,
            "pinboard"          => Origin::Pinboard,
            "other"             => Origin::Other,
            "manual"            => Origin::Manual,
            _                   => Origin::Manual,              // Default fallback
        }
    }
}

impl From<String> for DownloadStatus {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "queue"             => DownloadStatus::Queue,
            "backlog"           => DownloadStatus::Backlog,
            "done"              => DownloadStatus::Done,
            _                   => DownloadStatus::Queue,       // Default fallback
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OnDuplicate {
    Overwrite,
    CreateNew,
    DoNothing,
}

impl From<String> for OnDuplicate {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "overwrite"         => OnDuplicate::Overwrite,
            "create_new"        => OnDuplicate::CreateNew,
            "do_nothing"        => OnDuplicate::DoNothing,
            _                   => OnDuplicate::CreateNew,      // Default fallback
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutputFormat {
    #[serde(alias = "default")]
    Default,
    #[serde(alias = "audio")]
    Audio,
    #[serde(alias = "video")]
    Video,
}

impl From<String> for OutputFormat {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "audio" => OutputFormat::Audio,
            "video" => OutputFormat::Video,
            _        => OutputFormat::Default,
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeleteMode {
    Soft,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DefaultOutput {
    Audio,
    Video,
}

impl Default for DefaultOutput {
    fn default() -> Self { DefaultOutput::Video }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub id: Option<i64>,
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
    pub delete_mode: DeleteMode,
    pub debug_logs: bool,
    #[serde(default)]
    pub default_output: DefaultOutput,
    #[serde(default = "default_true")]
    pub download_automatically: bool,
    #[serde(default = "default_true")]
    pub keep_downloading_on_other_pages: bool,
}

fn default_true() -> bool { true }

/* ----------------------------- util: link normalize ----------------------------- */
fn normalize_link(mut s: String) -> String {
    // strip scheme
    if let Some(idx) = s.find("://") {
        s = s[idx + 3..].to_string();
    }
    // lowercase host part
    if let Some(idx) = s.find('/') {
        let (host, rest) = s.split_at(idx);
        s = format!("{}{}", host.to_lowercase(), rest);
    } else {
        s = s.to_lowercase();
    }
    // remove "www."
    if s.starts_with("www.") { s = s.trim_start_matches("www.").to_string(); }
    // drop query
    if let Some((base, _q)) = s.split_once('?') { s = base.to_string(); }
    // trim trailing slash
    while s.ends_with('/') { s.pop(); }
    s
}

/* -------------------------------- database -------------------------------- */
impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        let conn = Connection::open(&db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        let db = Database { conn };
        db.create_tables()?;
        Ok(db)
    }

    pub fn find_done_row_by_link(&self, link: &str) -> Result<Option<(i64, String)>> {
        let norm = normalize_link(link.to_string());
        let mut stmt = self.conn.prepare(
            "SELECT id, link, path
               FROM downloads
              WHERE status='done'
              ORDER BY id"
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64     = r.get(0)?;
            let db_link: String = r.get(1)?;
            let path: String    = r.get(2)?;
            if normalize_link(db_link) == norm {
                return Ok(Some((id, path)));
            }
        }
        Ok(None)
    }

    /// Hard-delete a row by id.
    pub fn delete_row_by_id(&self, id: i64) -> Result<usize> {
        let n = self.conn.execute("DELETE FROM downloads WHERE id=?1", [id])?;
        Ok(n)
    }

    /// Utility: ids and paths for all rows under a platform.
    pub fn list_ids_and_paths_by_platform(&self, platform: &str) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM downloads WHERE platform=?1")?;
        let mut rows = stmt.query([platform])?;
        let mut v = Vec::new();
        while let Some(r) = rows.next()? { v.push((r.get(0)?, r.get(1)?)); }
        Ok(v)
    }

    /// Utility: ids and paths for all rows in a collection.
    pub fn list_ids_and_paths_by_collection(&self, platform: &str, handle: &str, origin: &str) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM downloads WHERE platform=?1 AND user_handle=?2 AND origin=?3")?;
        let mut rows = stmt.query([platform, handle, origin])?;
        let mut v = Vec::new();
        while let Some(r) = rows.next()? { v.push((r.get(0)?, r.get(1)?)); }
        Ok(v)
    }

    /// Utility: ids and paths for all rows matching a link (any status).
    pub fn list_ids_and_paths_by_link(&self, link: &str) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM downloads WHERE link=?1")?;
        let mut rows = stmt.query([link])?;
        let mut v = Vec::new();
        while let Some(r) = rows.next()? { v.push((r.get(0)?, r.get(1)?)); }
        Ok(v)
    }

    /// Read the preferred output format for a link (first matching, priority queue/backlog). Returns "audio" | "video" | "default".
    pub fn output_format_for_link(&self, link: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT output_format FROM downloads WHERE link=?1 ORDER BY CASE status WHEN 'queue' THEN 0 WHEN 'backlog' THEN 1 ELSE 2 END, id LIMIT 1"
        )?;
        let mut rows = stmt.query([link])?;
        if let Some(r) = rows.next()? {
            let fmt: String = r.get(0)?;
            Ok(fmt)
        } else {
            Ok("default".to_string())
        }
    }

    /// Toggle a row's output_format between 'audio' and 'video'. If currently 'default', set to 'audio'.
    pub fn toggle_output_format_for_link(&self, link: &str) -> Result<usize> {
        // Find first matching row (any status); prefer queue/backlog
        let mut stmt = self.conn.prepare(
            "SELECT id, output_format FROM downloads WHERE link=?1 ORDER BY CASE status WHEN 'queue' THEN 0 WHEN 'backlog' THEN 1 ELSE 2 END, id LIMIT 1"
        )?;
        let mut rows = stmt.query([link])?;
        if let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let fmt: String = r.get(1)?;
            let next = match fmt.as_str() {
                "audio" => "video",
                "video" => "audio",
                _ => "audio",
            };
            let n = self.conn.execute("UPDATE downloads SET output_format=?1 WHERE id=?2", [next, &id.to_string()])?;
            Ok(n)
        } else {
            Ok(0)
        }
    }

    /// Explicitly set output_format for the row by link (first matching, priority queue/backlog).
    pub fn set_output_format_for_link(&self, link: &str, fmt: OutputFormat) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM downloads WHERE link=?1 ORDER BY CASE status WHEN 'queue' THEN 0 WHEN 'backlog' THEN 1 ELSE 2 END, id LIMIT 1"
        )?;
        let mut rows = stmt.query([link])?;
        if let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let n = self.conn.execute(
                "UPDATE downloads SET output_format=?1 WHERE id=?2",
                [format!("{:?}", fmt).to_lowercase(), id.to_string()],
            )?;
            Ok(n)
        } else {
            Ok(0)
        }
    }

    fn get_db_path() -> Result<PathBuf> {
        let config_dir = match dirs::config_dir() {
            Some(dir) => dir,
            None => return Err(rusqlite::Error::InvalidColumnName("Could not find config directory".to_string())),
        };

        let app_config_dir = config_dir.join("clip-downloader");
        std::fs::create_dir_all(&app_config_dir)
            .map_err(|_| rusqlite::Error::InvalidColumnName("Failed to create config directory".to_string()))?;

        Ok(app_config_dir.join("downloads.db"))
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL,
                name TEXT NOT NULL,
                media TEXT NOT NULL,
                user_handle TEXT NOT NULL,
                origin TEXT NOT NULL,
                link TEXT NOT NULL,
                output_format TEXT NOT NULL DEFAULT 'default' CHECK (output_format IN ('default','audio','video')),
                status TEXT NOT NULL,
                path TEXT NOT NULL,
                image_set_id TEXT,
                date_added TEXT NOT NULL,
                date_downloaded TEXT
            )",
            [],
        )?;


        // Settings
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                download_directory TEXT NOT NULL,
                on_duplicate TEXT NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO settings (id, download_directory, on_duplicate)
             VALUES (1, ?, ?)",
            [
                &dirs::download_dir()
                    .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                    .to_string_lossy()
                    .to_string(),
                "create_new"
            ],
        )?;
        Ok(())
    }

    /* ----------------------------- write helpers ----------------------------- */

    pub fn insert_download(&self, download: &Download) -> Result<i64> {
        let path_value = if download.path.is_empty() {
            "unknown_path".to_string()
        } else {
            download.path.clone()
        };

        self.conn.execute(
            "INSERT INTO downloads (platform, name, media, user_handle, origin, link, output_format, status, path, image_set_id, date_added, date_downloaded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            [
                &format!("{:?}", download.platform).to_lowercase(),
                &download.name,
                &format!("{:?}", download.media).to_lowercase(),
                &download.user,
                &format!("{:?}", download.origin).to_lowercase(),
                &download.link,
                &format!("{:?}", download.output_format).to_lowercase(),
                &format!("{:?}", download.status).to_lowercase(),
                &path_value,
                &download.image_set_id.clone().unwrap_or_default(),
                &download.date_added.to_rfc3339(),
                &download.date_downloaded.as_ref().map(|dt| dt.to_rfc3339()).unwrap_or_default(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Mark the first queued row for this link as done; set its path and date_downloaded.
    /// Uses a *loose* match (normalized URL) so minor URL differences (e.g. IG query params) still resolve.
    pub fn mark_link_done(&self, link: &str, path: &str) -> Result<usize> {
        let path_value = if path.is_empty() { "unknown_path".to_string() } else { path.to_string() };
        let now = Utc::now().to_rfc3339();
        let norm = normalize_link(link.to_string());

        // find oldest queued row whose normalized link matches
        let mut stmt = self.conn.prepare("SELECT id, link FROM downloads WHERE status='queue' ORDER BY id")?;
        let mut rows = stmt.query([])?;
        let mut target_id: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let db_link: String = row.get(1)?;
            if normalize_link(db_link) == norm {
                target_id = Some(id);
                break;
            }
        }

        let n = if let Some(id) = target_id {
            self.conn.execute(
                "UPDATE downloads SET status='done', path=?1, date_downloaded=?2 WHERE id=?3",
                params![path_value, now, id],
            )?
        } else {
            // fallback: strict equality (in case there is an exact match)
            self.conn.execute(
                "UPDATE downloads SET status='done', path=?2, date_downloaded=?3 WHERE link=?1 AND status='queue' LIMIT 1",
                [&link.to_string(), &path_value, &now],
            )?
        };

        Ok(n)
    }

    /* ------------------------------ read helpers ----------------------------- */

    /// Preferred collection (platform, origin, user_handle) for a given link.
    /// Priority: queue → backlog → done (oldest id first). Uses normalized-link matching.
    pub fn collection_for_link(&self, link: &str) -> Result<Option<CollectionInfo>> {
        let norm = normalize_link(link.to_string());
        let mut stmt = self.conn.prepare(
            "SELECT platform, origin, user_handle, link, status, id
               FROM downloads
              ORDER BY CASE status
                         WHEN 'queue' THEN 0
                         WHEN 'backlog' THEN 1
                         ELSE 2
                       END,
                       id"
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let platform: String    = r.get(0)?;
            let origin: String      = r.get(1)?;
            let user_handle: String = r.get(2)?;
            let db_link: String     = r.get(3)?;
            if normalize_link(db_link) == norm {
                return Ok(Some(CollectionInfo { platform, origin, user_handle }));
            }
        }
        Ok(None)
    }

    /// Compute a display-ready folder label: "{origin} - {user_handle}"
    /// Ensures both parts are non-empty (falls back to tokens if needed).
    pub fn collection_folder_label(origin: &str, user_handle: &str) -> String {
        let o = origin.trim();
        let u = user_handle.trim();
        let o = if o.is_empty() { "manual" } else { o };
        let u = if u.is_empty() || u.eq_ignore_ascii_case("unknown") { "Unknown" } else { u };
        format!("{o} - {u}")
    }

    /* -------------------------- UI-normalized listings -------------------------- */

    /// Fetch rows with `status = 'backlog'`, normalized for the UI.
    /// Ordered by platform → handle → type → name (case-insensitive).
    pub fn list_backlog_ui(&self) -> Result<Vec<UiBacklogRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT platform, user_handle, origin, media, link, name, output_format
             FROM downloads
             WHERE status = 'backlog'
             ORDER BY platform COLLATE NOCASE,
                      user_handle COLLATE NOCASE,
                      origin COLLATE NOCASE,
                      name COLLATE NOCASE",
        )?;

        let rows = stmt.query_map([], |row| {
            let platform: String      = row.get(0)?;
            let handle: String        = row.get(1)?;
            let origin: String        = row.get(2)?;
            let media: String         = row.get(3)?;
            let link: String          = row.get(4)?;
            let _name: String         = row.get(5)?;
            let output_format: String = row.get(6).unwrap_or_else(|_| "default".to_string());

            let content_type = match origin.as_str() {
                "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts" => origin.clone(),
                _ => "recommendation".to_string(),
            };
            let media_token = if media == "image" || media == "images" {
                "pictures".to_string()
            } else {
                "video".to_string()
            };

            Ok(UiBacklogRow { platform, content_type, handle, media: media_token, link, output_format })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Fetch rows with `status = 'queue'`, normalized for the UI.
    pub fn list_queue_ui(&self) -> Result<Vec<UiBacklogRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT platform, user_handle, origin, media, link, name, output_format
             FROM downloads
             WHERE status = 'queue'
             ORDER BY platform COLLATE NOCASE,
                      user_handle COLLATE NOCASE,
                      origin COLLATE NOCASE,
                      name COLLATE NOCASE",
        )?;

        let rows = stmt.query_map([], |row| {
            let platform: String      = row.get(0)?;
            let handle: String        = row.get(1)?;
            let origin: String        = row.get(2)?;
            let media: String         = row.get(3)?;
            let link: String          = row.get(4)?;
            let _name: String         = row.get(5)?;
            let output_format: String = row.get(6).unwrap_or_else(|_| "default".to_string());

            let content_type = match origin.as_str() {
                "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts" => origin.clone(),
                _ => "recommendation".to_string(),
            };
            let media_token = if media == "image" || media == "images" {
                "pictures".to_string()
            } else {
                "video".to_string()
            };

            Ok(UiBacklogRow { platform, content_type, handle, media: media_token, link, output_format })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_done_ui(&self) -> Result<Vec<UiBacklogRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT platform, user_handle, origin, media, link, name, output_format
             FROM downloads
             WHERE status = 'done'
             ORDER BY platform COLLATE NOCASE,
                      user_handle COLLATE NOCASE,
                      origin COLLATE NOCASE,
                      name COLLATE NOCASE",
        )?;

        let rows = stmt.query_map([], |row| {
            let platform: String      = row.get(0)?;
            let handle: String        = row.get(1)?;
            let origin: String        = row.get(2)?;
            let media: String         = row.get(3)?;
            let link: String          = row.get(4)?;
            let _name: String         = row.get(5)?;
            let output_format: String = row.get(6).unwrap_or_else(|_| "default".to_string());
    
            let content_type = match origin.as_str() {
                "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts" => origin.clone(),
                _ => "recommendation".to_string(),
            };
            let media_token = if media == "image" || media == "images" { "pictures".to_string() } else { "video".to_string() };
    
            Ok(UiBacklogRow { platform, content_type, handle, media: media_token, link, output_format })
        })?;
    
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    /* -------------------- status transitions (→ Queue) -------------------- */

    /// Move a single link from backlog to queue.
    pub fn move_link_to_queue(&self, link: &str) -> Result<usize> {
        let n = self.conn.execute("UPDATE downloads SET status='queue' WHERE link=?1 AND status='backlog'",[link])?;
        Ok(n)
    }

    /// Move all rows of a (platform, handle, origin) collection from backlog to queue.
    pub fn move_collection_to_queue(&self, platform: &str, handle: &str, origin: &str) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE downloads
               SET status='queue'
             WHERE platform    = ?1
               AND user_handle = ?2
               AND origin      = ?3
               AND status      = 'backlog'",
            [platform, handle, origin],
        )?;
        Ok(n)
    }

    /// Move all rows of a platform from backlog to queue.
    pub fn move_platform_to_queue(&self, platform: &str) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE downloads
               SET status='queue'
             WHERE platform = ?1
               AND status   = 'backlog'",
            [platform],
        )?;
        Ok(n)
    }

    /* -------------------- status transitions (→ Backlog) -------------------- */

    /// Move a single link from queue back to backlog.
    pub fn move_link_to_backlog(&self, link: &str) -> Result<usize> {
        let n = self.conn.execute("UPDATE downloads SET status='backlog' WHERE link=?1 AND status='queue'", [link])?;
        Ok(n)
    }

    /// Move all rows of a (platform, handle, origin) collection from queue back to backlog.
    pub fn move_collection_to_backlog(&self, platform: &str, handle: &str, origin: &str) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE downloads
               SET status='backlog'
             WHERE platform    = ?1
               AND user_handle = ?2
               AND origin      = ?3
               AND status      = 'queue'",
            [platform, handle, origin],
        )?;
        Ok(n)
    }

    /// Move all rows of a platform from queue back to backlog.
    pub fn move_platform_to_backlog(&self, platform: &str) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE downloads
               SET status='backlog'
             WHERE platform = ?1
               AND status   = 'queue'",
            [platform],
        )?;
        Ok(n)
    }
}
