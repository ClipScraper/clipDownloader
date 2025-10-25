use tauri::Emitter;

#[derive(serde::Serialize, Clone)]
pub struct DownloadResult {
    pub success: bool,
    pub message: String,
}

#[derive(serde::Serialize)]
pub(crate) struct StatusPayload {
    pub url: String,
    pub success: bool,
    pub message: String,
}

/// Send a `download-status` event to the frontend.
pub(crate) fn emit_status(window: &tauri::WebviewWindow, url: &str, success: bool, message: String) {
    let p = StatusPayload { url: url.to_string(), success, message };
    if let Err(e) = window.emit("download-status", &p) {
        eprintln!("[tauri] emit_status failed: {e}");
    }
}
