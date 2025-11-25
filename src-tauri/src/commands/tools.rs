use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SidecarCheck {
    pub yt_dlp: bool,
    pub gallery_dl: bool,
    pub ffmpeg: bool,
}

#[tauri::command]
pub async fn check_sidecar_tools(app: tauri::AppHandle) -> Result<SidecarCheck, String> {
    let _ = app; // not needed for system checks, keep signature stable

    // System PATH-based checks (not bundled sidecars)
    fn exists_in_dir(dir: &std::path::Path, name: &str) -> bool {
        let p = dir.join(name);
        if p.exists() {
            return true;
        }
        // Windows fallback
        let p_exe = dir.join(format!("{name}.exe"));
        p_exe.exists()
    }
    fn on_path(names: &[&str]) -> bool {
        use std::path::PathBuf;
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|v| {
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        // Common extras on macOS/Homebrew + standard bins
        for extra in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/local/sbin",
        ] {
            let pb = PathBuf::from(extra);
            if !dirs.iter().any(|d| d == &pb) {
                dirs.push(pb);
            }
        }
        for d in dirs.iter() {
            for n in names {
                if exists_in_dir(d, n) {
                    return true;
                }
            }
        }
        false
    }

    let yt_ok = on_path(&["yt-dlp", "yt_dlp", "yt-dlp_bin"]);
    let gal_ok = on_path(&["gallery-dl", "gallery_dl"]);
    let ffmpeg_ok = on_path(&["ffmpeg"]);

    Ok(SidecarCheck { yt_dlp: yt_ok, gallery_dl: gal_ok, ffmpeg: ffmpeg_ok })
}
