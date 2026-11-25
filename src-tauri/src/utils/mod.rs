use std::path::PathBuf;

pub mod os;
pub mod resource;
pub mod text;
pub mod url;

/// Resolve a path stored in the database to one that actually exists on disk.
///
/// Older builds mis-decoded yt-dlp's cp1252 stdout as UTF-8, corrupting any
/// non-ASCII characters in the stored download path (e.g. `Téléchargements`
/// became `T�l�chargements`). Only the download-root prefix is affected, so if
/// the stored path is missing we re-root the below-root components under the
/// correctly-encoded `download_directory` from settings.
pub(crate) fn resolve_stored_path(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    if p.exists() {
        return Some(p);
    }
    let root = PathBuf::from(crate::settings::load_settings().download_directory);
    let root_depth = root.components().count();
    let comps: Vec<_> = p.components().collect();
    if comps.len() > root_depth {
        let mut rebuilt = root;
        for c in &comps[root_depth..] {
            rebuilt.push(c.as_os_str());
        }
        if rebuilt.exists() {
            return Some(rebuilt);
        }
    }
    None
}
