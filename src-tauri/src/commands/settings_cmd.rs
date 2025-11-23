use crate::database::Settings;
use crate::download::manager::{DownloadCommand, DownloadManager};
use tauri::State;

#[tauri::command]
pub async fn load_settings() -> Settings {
    crate::settings::load_settings()
}

#[tauri::command]
pub async fn save_settings(
    manager: State<'_, DownloadManager>,
    settings: Settings,
) -> Result<(), String> {
    // persist first
    crate::settings::save_settings(&settings)?;

    // then live-toggle logging
    crate::logging::set_file_logging_enabled(settings.debug_logs);
    tracing::info!("settings saved; debug_logs now {}", settings.debug_logs);

    // notify download manager to refresh runtime parameters
    manager
        .send(DownloadCommand::RefreshSettings)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
