pub fn is_tiktok_photo(u: &str) -> bool {
    u.contains("tiktok.com/") && u.contains("/photo/")
}

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
    fn identifies_collection_cards_and_creator_routes_only_on_patreon() {
        for url in ["https://www.patreon.com/posts/101?collection=77", "https://patreon.com/c/creator/posts/a-title-101/", "https://www.patreon.com/creator/posts/a-title-101"] {
            assert_eq!(patreon_post_id(url).as_deref(), Some("101"));
        }
        for url in ["https://patreon.com.example.org/posts/101", "https://www.patreon.com/collection/101", "https://www.patreon.com/api/posts/101", "https://www.patreon.com/posts/hello"] {
            assert_eq!(patreon_post_id(url), None);
        }
    }
}
