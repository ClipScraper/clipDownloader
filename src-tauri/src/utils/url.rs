pub fn is_instagram_post(u: &str) -> bool {
    u.contains("instagram.com/") && u.contains("/p/")
}
pub fn is_tiktok_photo(u: &str) -> bool {
    u.contains("tiktok.com/") && u.contains("/photo/")
}
