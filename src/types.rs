use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Tiktok,
    Instagram,
    Youtube,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Liked,
    Reposts,
    Profile,
    Bookmarks,
    Playlist,
    Recommendation,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    #[serde(alias = "pictures")]
    Pictures,
    #[serde(alias = "video")]
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClipRow {
    #[serde(rename = "Platform")]
    pub platform: Platform,
    #[serde(rename = "Type")]
    pub content_type: ContentType,
    #[serde(rename = "Handle")]
    pub handle: String,
    #[serde(rename = "Media")]
    pub media: MediaKind,
    #[serde(rename = "link")]
    pub link: String,
    /// Comes from DB; optional when deserializing CSV.
    #[serde(default)]
    pub name: String,
}

pub fn platform_str(p: &Platform) -> &'static str {
    match p {
        Platform::Tiktok => "tiktok",
        Platform::Instagram => "instagram",
        Platform::Youtube => "youtube",
    }
}

pub fn content_type_str(t: &ContentType) -> &'static str {
    match t {
        ContentType::Liked => "liked",
        ContentType::Reposts => "reposts",
        ContentType::Profile => "profile",
        ContentType::Bookmarks => "bookmarks",
        ContentType::Playlist => "playlist",
        ContentType::Recommendation => "recommendation",
        ContentType::Other => "other",
    }
}
