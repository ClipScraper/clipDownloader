use crate::database::Settings;

#[tauri::command]
pub async fn load_settings() -> Settings {
    crate::settings::load_settings()
}

#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<(), String> {
    crate::settings::save_settings(&settings)
}
