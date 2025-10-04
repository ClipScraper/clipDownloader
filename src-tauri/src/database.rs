use chrono::{DateTime, Utc};
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub id: Option<i64>,
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OnDuplicate {
    Overwrite,
    CreateNew,
    DoNothing,
}

impl From<String> for Platform {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "youtube" => Platform::Youtube,
            "tiktok" => Platform::Tiktok,
            "instagram" => Platform::Instagram,
            _ => Platform::Youtube, // Default fallback
        }
    }
}

impl From<String> for MediaKind {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "image" | "images" => MediaKind::Image,
            "video" | "videos" => MediaKind::Video,
            _ => MediaKind::Video, // Default fallback
        }
    }
}

impl From<String> for Origin {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "recommendation" => Origin::Recommendation,
            "playlist" => Origin::Playlist,
            "profile" => Origin::Profile,
            "bookmarks" => Origin::Bookmarks,
            "other" => Origin::Other,
            "manual" => Origin::Manual,
            _ => Origin::Manual, // Default fallback
        }
    }
}

impl From<String> for DownloadStatus {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "queue" => DownloadStatus::Queue,
            "backlog" => DownloadStatus::Backlog,
            "done" => DownloadStatus::Done,
            _ => DownloadStatus::Queue, // Default fallback
        }
    }
}

impl From<String> for OnDuplicate {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "overwrite" => OnDuplicate::Overwrite,
            "create_new" => OnDuplicate::CreateNew,
            "do_nothing" => OnDuplicate::DoNothing,
            _ => OnDuplicate::CreateNew, // Default fallback
        }
    }
}

pub struct Database {
    conn: Connection,
}

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

        self.migrate_add_path_column()?;
        self.migrate_add_image_set_id_column()?;
        self.migrate_remove_link_unique_constraint()?;

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
        // Use PRAGMA table_info to check if the image_set_id column exists
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| {
            Ok(row.get::<_, String>(1)?) // column name is in index 1
        })?;

        let mut column_names = Vec::new();
        for column in columns {
            column_names.push(column?);
        }

        let has_image_set_id_column = column_names.iter().any(|name| name == "image_set_id");

        if !has_image_set_id_column {
            // Add the image_set_id column if it doesn't exist
            self.conn.execute("ALTER TABLE downloads ADD COLUMN image_set_id TEXT", [])?;
        }

        Ok(())
    }

    /// Migration: Remove UNIQUE constraint from link column if it exists
    fn migrate_remove_link_unique_constraint(&self) -> Result<()> {
        // For existing databases, we need to recreate the table without the UNIQUE constraint
        // We'll check if we can successfully insert duplicate links, and if not, recreate the table

        // First, let's try to see if the constraint exists by checking the table schema
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| {
            Ok(row.get::<_, String>(1)?) // column name is in index 1
        })?;

        let column_names: Vec<String> = columns.collect::<Result<Vec<_>, _>>()?;
        let has_link_column = column_names.iter().any(|name| name == "link");

        if has_link_column {
            // Try to detect if there's a UNIQUE constraint by attempting to get the table schema
            let mut stmt = self.conn.prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='downloads'")?;
            let schema: Option<String> = stmt.query_row([], |row| row.get(0)).unwrap_or(None);

            if let Some(sql) = schema {
                if sql.contains("UNIQUE") && sql.contains("link") {
                    // The table has a UNIQUE constraint on link, recreate it
                    self.recreate_downloads_table_without_unique_constraint()?;
                }
            }
        }

        Ok(())
    }

    /// Helper function to recreate the downloads table without UNIQUE constraint
    fn recreate_downloads_table_without_unique_constraint(&self) -> Result<()> {
        // Create new table without UNIQUE constraint on link
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

        // Copy data from old table to new table
        self.conn.execute(
            "INSERT INTO downloads_new (id, platform, name, media, user_handle, origin, link, status, path, image_set_id, date_added, date_downloaded)
             SELECT id, platform, name, media, user_handle, origin, link, status, path, image_set_id, date_added, date_downloaded
             FROM downloads",
            [],
        )?;

        // Drop old table
        self.conn.execute("DROP TABLE downloads", [])?;

        // Rename new table
        self.conn.execute("ALTER TABLE downloads_new RENAME TO downloads", [])?;

        Ok(())
    }

    /// Migration: Add path column to existing databases that don't have it
    fn migrate_add_path_column(&self) -> Result<()> {
        // Use PRAGMA table_info to check if the path column exists
        let mut stmt = self.conn.prepare("PRAGMA table_info(downloads)")?;
        let columns = stmt.query_map([], |row| {
            Ok(row.get::<_, String>(1)?) // column name is in index 1
        })?;

        let mut column_names = Vec::new();
        for column in columns {
            column_names.push(column?);
        }

        let has_path_column = column_names.iter().any(|name| name == "path");

        if !has_path_column {
            // Add the path column if it doesn't exist
            self.conn.execute(
                "ALTER TABLE downloads ADD COLUMN path TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }

        Ok(())
    }

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

    pub fn get_settings(&self) -> Result<Settings> {
        let mut stmt = self.conn.prepare("SELECT download_directory, on_duplicate FROM settings WHERE id = 1")?;
        let settings = stmt.query_row([], |row| {
            Ok(Settings {
                id: Some(1),
                download_directory: row.get(0)?,
                on_duplicate: OnDuplicate::from(row.get::<_, String>(1)?),
            })
        })?;

        Ok(settings)
    }

    pub fn update_settings(&self, settings: &Settings) -> Result<()> {
        self.conn.execute(
            "UPDATE settings SET download_directory = ?1, on_duplicate = ?2 WHERE id = 1",
            [&settings.download_directory, &format!("{:?}", settings.on_duplicate).to_lowercase()],
        )?;
        Ok(())
    }
}
