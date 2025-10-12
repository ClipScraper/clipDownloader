use crate::database::Settings;

#[tauri::command]
pub async fn load_settings() -> Settings {
    crate::settings::load_settings()
}

#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<(), String> {
    // persist first
    crate::settings::save_settings(&settings)?;

    // then live-toggle logging
    crate::logging::set_file_logging_enabled(settings.debug_logs);
    tracing::info!("settings saved; debug_logs now {}", settings.debug_logs);

    Ok(())
}
