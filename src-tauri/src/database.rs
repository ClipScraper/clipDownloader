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


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub id: Option<i64>,
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
}

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

    /// Migration: Add image_set_id column to existing databases that don't have it
    fn migrate_add_image_set_id_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?;
        let mut column_names = Vec::new();
        for column in columns {
            column_names.push(column?);
        }
        if !column_names.iter().any(|n| n == "image_set_id") {
            self.conn.execute("ALTER TABLE downloads ADD COLUMN image_set_id TEXT", [])?;
        }
        Ok(())
    }

    /// Migration: Remove UNIQUE constraint from link column if it exists
    fn migrate_remove_link_unique_constraint(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?;
        let column_names: Vec<String> = columns.collect::<Result<Vec<_>, _>>()?;
        if column_names.iter().any(|n| n == "link") {
            let mut stmt = self.conn.prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='downloads'")?;
            let schema: Option<String> = stmt.query_row([], |row| row.get(0)).unwrap_or(None);
            if let Some(sql) = schema {
                if sql.contains("UNIQUE") && sql.contains("link") {
                    self.recreate_downloads_table_without_unique_constraint()?;
                }
            }
        }
        Ok(())
    }

    fn recreate_downloads_table_without_unique_constraint(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE downloads_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL,
                name TEXT NOT NULL,
                media TEXT NOT NULL,
                user_handle TEXT NOT NULL,
                origin TEXT NOT NULL,
                link TEXT NOT NULL,
                status TEXT NOT NULL,
                path TEXT NOT NULL,
                image_set_id TEXT,
                date_added TEXT NOT NULL,
                date_downloaded TEXT
            )",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO downloads_new (id, platform, name, media, user_handle, origin, link, status, path, image_set_id, date_added, date_downloaded)
             SELECT id, platform, name, media, user_handle, origin, link, status, path, image_set_id, date_added, date_downloaded
             FROM downloads",
            [],
        )?;
        self.conn.execute("DROP TABLE downloads", [])?;
        self.conn.execute("ALTER TABLE downloads_new RENAME TO downloads", [])?;
        Ok(())
    }

    /// Migration: Add path column to existing databases that don't have it
    fn migrate_add_path_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?;
        let mut column_names = Vec::new();
        for column in columns {
            column_names.push(column?);
        }
        if !column_names.iter().any(|n| n == "path") {
            self.conn.execute("ALTER TABLE downloads ADD COLUMN path TEXT NOT NULL DEFAULT ''", [])?;
        }
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
            "INSERT INTO downloads (platform, name, media, user_handle, origin, link, status, path, image_set_id, date_added, date_downloaded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            [
                &format!("{:?}", download.platform).to_lowercase(),
                &download.name,
                &format!("{:?}", download.media).to_lowercase(),
                &download.user,
                &format!("{:?}", download.origin).to_lowercase(),
                &download.link,
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
            "SELECT platform, user_handle, origin, media, link, name
             FROM downloads
             WHERE status = 'backlog'
             ORDER BY platform COLLATE NOCASE,
                      user_handle COLLATE NOCASE,
                      origin COLLATE NOCASE,
                      name COLLATE NOCASE",
        )?;

        let rows = stmt.query_map([], |row| {
            let platform: String = row.get(0)?;
            let handle: String   = row.get(1)?;
            let origin: String   = row.get(2)?;
            let media: String    = row.get(3)?;
            let link: String     = row.get(4)?;
            let _name: String    = row.get(5)?;

            let content_type = match origin.as_str() {
                "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts" => origin.clone(),
                _ => "recommendation".to_string(),
            };
            let media_token = if media == "image" || media == "images" {
                "pictures".to_string()
            } else {
                "video".to_string()
            };

            Ok(UiBacklogRow { platform, content_type, handle, media: media_token, link })
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
            "SELECT platform, user_handle, origin, media, link, name
             FROM downloads
             WHERE status = 'queue'
             ORDER BY platform COLLATE NOCASE,
                      user_handle COLLATE NOCASE,
                      origin COLLATE NOCASE,
                      name COLLATE NOCASE",
        )?;

        let rows = stmt.query_map([], |row| {
            let platform: String = row.get(0)?;
            let handle: String   = row.get(1)?;
            let origin: String   = row.get(2)?;
            let media: String    = row.get(3)?;
            let link: String     = row.get(4)?;
            let _name: String    = row.get(5)?;

            let content_type = match origin.as_str() {
                "recommendation" | "playlist" | "profile" | "bookmarks" | "liked" | "reposts" => origin.clone(),
                _ => "recommendation".to_string(),
            };
            let media_token = if media == "image" || media == "images" {
                "pictures".to_string()
            } else {
                "video".to_string()
            };

            Ok(UiBacklogRow { platform, content_type, handle, media: media_token, link })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
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
}
