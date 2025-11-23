use serde::Serialize;
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Serialize)]
pub struct SidecarCheck {
    pub yt_dlp: bool,
    pub gallery_dl: bool,
    pub ffmpeg: bool,
}

#[tauri::command]
pub async fn check_sidecar_tools(app: tauri::AppHandle) -> Result<SidecarCheck, String> {
    // Check sidecar executables presence
    let yt_ok = app.shell().sidecar("yt-dlp").is_ok();
    let gal_ok = app.shell().sidecar("gallery-dl").is_ok();

    // Check ffmpeg presence in the bundled resources dir (used via --ffmpeg-location)
    use tauri::path::BaseDirectory;
    let res_dir = app
        .path()
        .resolve("", BaseDirectory::Resource)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let mut ffmpeg_ok = res_dir.join("ffmpeg").exists();
    if !ffmpeg_ok {
        ffmpeg_ok = res_dir.join("ffmpeg.exe").exists();
    }
    if !ffmpeg_ok {
        // Some packages ship ffmpeg under bin/
        ffmpeg_ok = res_dir.join("bin").join("ffmpeg").exists()
            || res_dir.join("bin").join("ffmpeg.exe").exists();
    }

    Ok(SidecarCheck {
        yt_dlp: yt_ok,
        gallery_dl: gal_ok,
        ffmpeg: ffmpeg_ok,
    })
}


