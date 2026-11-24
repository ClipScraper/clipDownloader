pub fn is_tiktok_photo(u: &str) -> bool {
    u.contains("tiktok.com/") && u.contains("/photo/")
}

/// Only network URLs may reach the downloader command line.
pub fn validate_download_url(raw: &str) -> Result<(), String> {
    let url = tauri::Url::parse(raw).map_err(|_| "Enter an absolute HTTP or HTTPS URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Only HTTP and HTTPS downloads are supported".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Download URLs must not contain credentials".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_urls_cannot_be_options_or_local_files() {
        for value in ["--exec=malicious", "file:///tmp/private", "/tmp/file", "ftp://example.com/video", "https://user:pass@example.com"] {
            assert!(validate_download_url(value).is_err(), "{value}");
        }
        assert!(validate_download_url("https://youtube.com/watch?v=one&list=two").is_ok());
    }
}
