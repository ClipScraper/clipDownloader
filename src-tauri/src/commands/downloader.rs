use crate::database::{
    Database, Download, DownloadStatus, MediaKind, Origin, OutputFormat, Platform,
};
use crate::download::manager::{DownloadCommand, DownloadManager, DownloadOverrides};
use chrono::Utc;
use tauri::State;

#[tauri::command]
pub async fn download_url(
    manager: State<'_, DownloadManager>,
    url: String,
    output_format: Option<String>,
    outputFormat: Option<String>,
    flat_destination: Option<bool>,
    flatDestination: Option<bool>,
) -> Result<i64, String> {
    let force_audio =
        output_format
            .or(outputFormat)
            .and_then(|fmt| match fmt.to_lowercase().as_str() {
                "audio" => Some(true),
                "video" => Some(false),
                _ => None,
            });
    let flat = flat_destination.or(flatDestination).unwrap_or(false);

    let cleaned_url = sanitize_url(&url);
    let lookup_url = cleaned_url.clone();
    let force_audio_clone = force_audio.clone();
    let (row_id, created) = tauri::async_runtime::spawn_blocking(move || {
        ensure_row_for_url(&lookup_url, force_audio_clone)
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;

    if created {
        println!(
            "[DOWNLOADER] inserted new manual row {} for {}",
            row_id, cleaned_url
        );
    }

    manager
        .send(DownloadCommand::StartNow {
            id: row_id,
            overrides: Some(DownloadOverrides {
                force_audio,
                flat_destination: flat,
            }),
        })
        .await?;

    Ok(row_id)
}

#[tauri::command]
pub async fn cancel_download(manager: State<'_, DownloadManager>, id: i64) -> Result<(), String> {
    manager
        .send(DownloadCommand::Cancel { id })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn enqueue_downloads(
    manager: State<'_, DownloadManager>,
    ids: Vec<i64>,
) -> Result<(), String> {
    manager
        .send(DownloadCommand::Enqueue { ids })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn move_downloads_to_backlog(
    manager: State<'_, DownloadManager>,
    ids: Vec<i64>,
) -> Result<(), String> {
    manager
        .send(DownloadCommand::MoveToBacklog { ids })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_download_paused(
    manager: State<'_, DownloadManager>,
    paused: bool,
) -> Result<(), String> {
    manager
        .send(DownloadCommand::SetPaused(paused))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_download_settings(manager: State<'_, DownloadManager>) -> Result<(), String> {
    manager
        .send(DownloadCommand::RefreshSettings)
        .await
        .map_err(|e| e.to_string())
}

fn sanitize_url(raw: &str) -> String {
    raw.trim()
        .replace("#__audio_only__", "")
        .replace("#__flat__", "")
}

fn ensure_row_for_url(url: &str, force_audio: Option<bool>) -> Result<(i64, bool), String> {
    let db = Database::new().map_err(|e| e.to_string())?;
    if let Some(id) = db.find_id_by_link(url).map_err(|e| e.to_string())? {
        return Ok((id, false));
    }

    let platform = infer_platform(url);
    let media_kind = infer_media(url);
    let output = match force_audio {
        Some(true) => OutputFormat::Audio,
        Some(false) => OutputFormat::Video,
        None => OutputFormat::Default,
    };

    let download = Download {
        id: None,
        platform,
        name: url.to_string(),
        media: media_kind,
        user: "Unknown".into(),
        origin: Origin::Manual,
        link: url.to_string(),
        output_format: output,
        status: DownloadStatus::Queued,
        path: "unknown_path".into(),
        image_set_id: None,
        date_added: Utc::now(),
        date_downloaded: None,
    };
    let id = db.insert_download(&download).map_err(|e| e.to_string())?;
    Ok((id, true))
}

fn infer_platform(url: &str) -> Platform {
    if url.contains("instagram.com") {
        Platform::Instagram
    } else if url.contains("tiktok.com") {
        Platform::Tiktok
    } else if url.contains("pinterest.com") || url.contains("pin.it") {
        Platform::Pinterest
    } else {
        Platform::Youtube
    }
}

fn infer_media(url: &str) -> MediaKind {
    if url.contains("/photo/") || url.contains("pinterest.com") {
        MediaKind::Image
    } else {
        MediaKind::Video
    }
}
