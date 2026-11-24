use crate::database::MediaKind;
use crate::utils::url::{link_service, patreon_post_id};
use tauri::Url;

/// Classify the destination, independently of which collection contained it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Image,
    Video,
    File,
    FileEndpoint,
    Gallery,
    InstagramPost,
    PatreonPost,
    Reference,
}

impl Resource {
    pub fn media(self) -> MediaKind {
        match self {
            Self::Image | Self::Gallery | Self::InstagramPost => MediaKind::Image,
            Self::Video => MediaKind::Video,
            Self::File | Self::FileEndpoint | Self::PatreonPost => MediaKind::File,
            Self::Reference => MediaKind::Link,
        }
    }
}

pub fn classify_url(url: &Url, hint: &str) -> Resource {
    use Resource::*;
    let host = url.host_str().unwrap_or("").trim_end_matches('.');
    let service = link_service(url);
    let path = url.path().trim_matches('/');
    let parts: Vec<_> = path.split('/').collect();
    let parts = parts.as_slice();

    match service {
        "mega" | "telegram" => return Reference,
        "dropbox" => {
            let direct = host == "dl.dropbox.com" || host == "dropboxusercontent.com" || host.ends_with(".dropboxusercontent.com")
                || url.query_pairs().any(|(key, value)| (key == "dl" || key == "raw") && value == "1");
            if !direct { return Reference; }
        }
        _ => {}
    }
    if service == "patreon" && patreon_post_id(url.as_str()).is_some() { return PatreonPost; }

    // Only the path names the file. Keep signatures, keys and fragments on the original URL.
    let extension = path.rsplit('/').next().and_then(|name| name.rsplit_once('.')).map(|(_, ext)| ext.to_ascii_lowercase());
    match extension.as_deref() {
        Some("jpg" | "jpeg" | "jpe" | "gif" | "png" | "webp" | "avif" | "bmp" | "svg" | "tif" | "tiff" | "heif" | "heic") => return Image,
        Some("mp4" | "m4v" | "mov" | "webm" | "mkv" | "m3u8" | "mpd" | "ogv" | "avi" | "mpg" | "mpeg") => return Video,
        Some("zip" | "7z" | "rar" | "pdf" | "epub" | "mp3" | "m4a" | "wav" | "flac" | "ogg" | "tar" | "gz" | "txt" | "csv" | "docx" | "xlsx" | "blend" | "psd" | "stl") => return File,
        _ => {}
    }

    match service {
        "instagram" => match parts {
            ["p", _] | [_, "p", _] => InstagramPost,
            ["reel" | "reels" | "tv", _] | [_, "reel" | "reels" | "tv", _] => Video,
            _ => Reference,
        },
        "tiktok" => match parts {
            [_, "photo", _] => Gallery,
            [_, "video", _] | ["t" | "v", _] => Video,
            _ if matches!(host, "vm.tiktok.com" | "vt.tiktok.com") && !path.is_empty() => Video,
            _ => Reference,
        },
        "youtube" => match parts {
            ["watch"] if url.query_pairs().any(|(k, v)| k == "v" && !v.is_empty()) => Video,
            ["shorts" | "embed" | "live", _] => Video,
            ["playlist"] if url.query_pairs().any(|(k, v)| k == "list" && !v.is_empty()) => Video,
            [_] if host == "youtu.be" && !path.is_empty() => Video,
            _ => Reference,
        },
        "pinterest" if matches!(parts, ["pin", _]) || (host == "pin.it" && !path.is_empty()) => Gallery,
        "twitter" if parts.windows(2).any(|p| p[0] == "status" && !p[1].is_empty() && p[1].bytes().all(|b| b.is_ascii_digit())) => Video,
        "patreon" if path == "file" => File,
        "dropbox" => File,
        "twitter" | "pinterest" | "patreon" => Reference,
        _ if matches!(host, "player.vimeo.com" | "stream.mux.com")
            || (matches!(host, "vimeo.com" | "www.vimeo.com") && parts.last().is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))) => Video,
        _ => match hint.to_ascii_lowercase().as_str() {
            "pictures" | "picture" | "image" | "images" => Image,
            "video" | "videos" => Video,
            _ if matches!(parts.last(), Some(&"download" | &"attachment")) || (parts.last() == Some(&"file") && url.query().is_some()) => FileEndpoint,
            "file" | "attachment" => File,
            "manual" => Video,
            _ => Reference,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_destination_even_when_a_collection_supplies_a_stale_hint() {
        for (raw, hint, expected) in [
            ("https://c10.patreonusercontent.com/4/patreon-media/p/post/123/hash/size/1.gif?token-hash=sample%3D&token-time=123", "link", Resource::Image),
            ("https://example.org/animation.GIF?download=1", "video", Resource::Image),
            ("https://www.instagram.com/reel/example/", "link", Resource::Video),
            ("https://www.instagram.com/creator/p/abc/", "link", Resource::InstagramPost),
            ("https://www.tiktok.com/@creator/video/123", "link", Resource::Video),
            ("https://www.tiktok.com/@creator/photo/123", "link", Resource::Gallery),
            ("https://vm.tiktok.com/share/", "link", Resource::Video),
            ("https://www.youtube.com/shorts/test", "link", Resource::Video),
            ("https://www.patreon.com/posts/123?collection=77", "link", Resource::PatreonPost),
            ("https://www.pinterest.com/pin/123/", "link", Resource::Gallery),
            ("https://x.com/user/status/123", "unknown", Resource::Video),
            ("https://x.com/user", "unknown", Resource::Reference),
            ("https://mega.nz/file/abc#example", "video", Resource::Reference),
            ("https://t.me/+example", "video", Resource::Reference),
            ("https://www.dropbox.com/scl/fi/id/video.mp4?dl=0", "video", Resource::Reference),
            ("https://www.dropbox.com/scl/fi/id/photo.gif?dl=1", "link", Resource::Image),
            ("https://example.org/clip?file=1", "pictures", Resource::Image),
            ("https://example.org/read?filename=photo.gif", "link", Resource::Reference),
            ("https://instagram.com.example.org/reel/abc", "link", Resource::Reference),
        ] {
            assert_eq!(classify_url(&Url::parse(raw).unwrap(), hint), expected, "{raw}");
        }
    }
}
