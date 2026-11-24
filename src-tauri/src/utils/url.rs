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
