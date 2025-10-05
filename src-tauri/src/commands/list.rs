use tauri::command;

/// Return all backlog rows, already normalized for the UI.
#[command]
pub async fn list_backlog() -> Result<Vec<crate::database::UiBacklogRow>, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    db.list_backlog_ui().map_err(|e| e.to_string())
}
