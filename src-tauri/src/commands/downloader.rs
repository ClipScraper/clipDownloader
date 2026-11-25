use crate::database::{Database, Download, DownloadStatus, Origin, OutputFormat, Platform};
use crate::download::manager::{DownloadCommand, DownloadManager, DownloadOverrides};
use chrono::Utc;
use tauri::{Emitter, State};
use crate::utils::{resource::{classify_url, Resource}, url::{http_url, link_service}};

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum Submission {
    Download(i64),
    Notice(&'static str),
}

#[tauri::command]
pub async fn download_url(app: tauri::AppHandle, manager: State<'_, DownloadManager>, url: String, output_format: Option<String>, outputFormat: Option<String>, flat_destination: Option<bool>, flatDestination: Option<bool>) -> Result<Submission, String> {
    let force_audio = output_format.or(outputFormat).and_then(|fmt| match fmt.to_lowercase().as_str() {
        "audio" => Some(true),
        "video" => Some(false),
        _ => None,
    });
    let flat = flat_destination.or(flatDestination).unwrap_or(false);

    let cleaned_url = sanitize_url(&url);
    let result = tauri::async_runtime::spawn_blocking(move || ensure_row_for_url(&cleaned_url, force_audio)).await.map_err(|e| format!("Join error: {e}"))??;
    let Submission::Download(row_id) = result else {
        let _ = app.emit("post-links-collected", ());
        return Ok(result);
    };

    manager.send(DownloadCommand::StartNow {id: row_id, overrides: Some(DownloadOverrides {force_audio, flat_destination: flat})}).await?;

    Ok(Submission::Download(row_id))
}

#[tauri::command]
pub async fn cancel_download(manager: State<'_, DownloadManager>, id: i64) -> Result<(), String> {
    manager.send(DownloadCommand::Cancel {id}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn enqueue_downloads(manager: State<'_, DownloadManager>, ids: Vec<i64>) -> Result<(), String> {
    manager.send(DownloadCommand::Enqueue {ids}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn move_downloads_to_backlog(manager: State<'_, DownloadManager>, ids: Vec<i64>) -> Result<(), String> {
    manager.send(DownloadCommand::MoveToBacklog {ids}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_download_paused(manager: State<'_, DownloadManager>, paused: bool) -> Result<(), String> {
    manager.send(DownloadCommand::SetPaused(paused)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_download_settings(manager: State<'_, DownloadManager>) -> Result<(), String> {
    manager.send(DownloadCommand::RefreshSettings).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reconcile_downloads(manager: State<'_, DownloadManager>) -> Result<(), String> {
    manager.send(DownloadCommand::ReconcileState).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_downloads_snapshot(manager: State<'_, DownloadManager>) -> Result<Vec<crate::database::UiBacklogRow>, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    manager.send(DownloadCommand::RefreshSnapshot {reply: reply_tx}).await.map_err(|e| e.to_string())?;
    reply_rx.await.map_err(|e| format!("snapshot channel closed: {e}"))?
}

fn sanitize_url(raw: &str) -> String {
    raw.trim().trim_end_matches("#__flat__").trim_end_matches("#__audio_only__").to_owned()
}

fn ensure_row_for_url(url: &str, force_audio: Option<bool>) -> Result<Submission, String> {
    let parsed = http_url(url)?;
    let db = Database::new().map_err(|e| e.to_string())?;
    // An explicit manual submission can still use yt-dlp's generic extractor on unknown sites.
    let service = link_service(&parsed);
    let route = classify_url(&parsed, if service == "other" { "manual" } else { "" });
    if let Some(id) = db.find_id_by_link(url).map_err(|e| e.to_string())? {
        let row = db.find_download_by_id(id).map_err(|e| e.to_string())?.ok_or("Item no longer exists")?;
        return Ok(match row.status {
            DownloadStatus::Reference => Submission::Notice("Saved in Links"),
            DownloadStatus::Done if route == Resource::Reference => Submission::Notice("Already in Library"),
            _ => Submission::Download(id),
        });
    }
    let platform = Platform::from(service.to_owned());
    let media_kind = route.media();
    let status = if route == Resource::Reference { DownloadStatus::Reference } else { DownloadStatus::Queued };
    let output = match force_audio {
        Some(true) => OutputFormat::Audio,
        Some(false) => OutputFormat::Video,
        None => OutputFormat::Default,
    };

    let download = Download {id: None, platform, name: url.to_string(), media: media_kind, user: "Unknown".into(), origin: Origin::Manual, link: url.to_string(), output_format: output, status, path: "unknown_path".into(), image_set_id: None, last_error: None, date_added: Utc::now(), date_downloaded: None};
    let id = db.insert_download(&download).map_err(|e| e.to_string())?;
    Ok(if route == Resource::Reference { Submission::Notice("Saved in Links") } else { Submission::Download(id) })
}
