pub fn is_tiktok_photo(u: &str) -> bool {
    u.contains("tiktok.com/") && u.contains("/photo/")
}
