pub fn http_url(raw: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(raw).map_err(|_| "Invalid download URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("Expected an HTTP or HTTPS URL without embedded credentials".into());
    }
    Ok(url)
}

/// Both numeric collection cards and creator-prefixed, titled permalinks identify a single post.
pub fn patreon_post_id(raw: &str) -> Option<String> {
    let url = http_url(raw).ok()?;
    if !matches!(url.host_str()?, "www.patreon.com" | "patreon.com") {
        return None;
    }
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let slug = match parts.as_slice() {
        ["posts", slug] | ["c", _, "posts", slug] => slug,
        [creator, "posts", slug] if *creator != "api" => slug,
        _ => return None,
    };
    let id = slug.rsplit('-').next()?;
    (!id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_destination_hosts_and_preserves_browser_targets() {
        for (raw, expected) in [
            ("https://WWW.Instagram.com/p/1", "instagram"), ("https://mobile.twitter.com/user/status/2", "twitter"),
            ("https://x.com/user", "twitter"), ("https://mega.nz/file/id#key", "mega"),
            ("https://dl.dropboxusercontent.com/file", "dropbox"), ("https://t.me/+group", "telegram"),
            ("https://web.telegram.org/a/", "telegram"), ("https://mega.nz.example.org/", "other"),
            ("https://example.org/?next=telegram.me", "other"),
        ] {
            assert_eq!(link_service(&http_url(raw).unwrap()), expected);
        }
        assert_eq!(text_link_url("mega.nz/file/id#Ab+Cd").unwrap().as_str(), "https://mega.nz/file/id#Ab+Cd");
        assert!(text_link_url("a.filename").is_none());
        for bad in ["file:///tmp/test", "javascript:alert(1)", "https://user:pass@example.org", "tg://resolve?domain=example"] {
            assert!(http_url(bad).is_err());
        }
    }

    #[test]
    fn identifies_collection_cards_and_creator_routes_only_on_patreon() {
        for url in ["https://www.patreon.com/posts/101?collection=77", "https://patreon.com/c/creator/posts/a-title-101/", "https://www.patreon.com/creator/posts/a-title-101"] {
            assert_eq!(patreon_post_id(url).as_deref(), Some("101"));
        }
        for url in ["https://patreon.com.example.org/posts/101", "https://www.patreon.com/collection/101", "https://www.patreon.com/api/posts/101", "https://www.patreon.com/posts/hello"] {
            assert_eq!(patreon_post_id(url), None);
        }
    }
}

pub fn link_service(url: &tauri::Url) -> &'static str {
    let host = url.host_str().unwrap_or("").trim_end_matches('.');
    let domains: &[(&str, &[&str])] = &[
        ("instagram", &["instagram.com", "instagr.am", "ig.me"]),
        ("twitter", &["x.com", "twitter.com", "t.co"]),
        ("mega", &["mega.nz", "mega.co.nz", "mega.io", "mega.app"]),
        ("dropbox", &["dropbox.com", "dropboxusercontent.com", "db.tt"]),
        ("telegram", &["t.me", "telegram.me", "telegram.org", "telegram.dog"]),
        ("tiktok", &["tiktok.com"]), ("youtube", &["youtube.com", "youtu.be", "youtube-nocookie.com"]),
        ("pinterest", &["pinterest.com", "pin.it"]), ("patreon", &["patreon.com"]),
    ];
    for &(service, domains) in domains {
        if domains.iter().any(|domain| host == *domain || host.strip_suffix(domain).is_some_and(|prefix| prefix.ends_with('.'))) {
            return service;
        }
    }
    "other"
}

pub fn text_link_url(raw: &str) -> Option<tauri::Url> {
    if let Ok(url) = http_url(raw) { return Some(url); }
    // Recognize service URLs creators often paste without a scheme. Keep other prose as text.
    let url = http_url(&format!("https://{raw}")).ok()?;
    (link_service(&url) != "other").then_some(url)
}
