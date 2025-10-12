use serde_json::Value;

#[tauri::command]
pub async fn frontend_log(level: String, message: String, context: Option<Value>) -> Result<(), String> {
    let ctx_str = context
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok())
        .unwrap_or_else(|| "{}".to_string());

    match level.to_lowercase().as_str() {
        "error" => tracing::error!(target: "frontend", context=%ctx_str, "{message}"),
        "warn" | "warning" => tracing::warn!(target: "frontend", context=%ctx_str, "{message}"),
        "debug" => tracing::debug!(target: "frontend", context=%ctx_str, "{message}"),
        "trace" => tracing::trace!(target: "frontend", context=%ctx_str, "{message}"),
        "info" => tracing::info!(target: "frontend", context=%ctx_str, "{message}"),
        other => tracing::info!(target: "frontend", level=%other, context=%ctx_str, "{message}"),
    }
    Ok(())
}
