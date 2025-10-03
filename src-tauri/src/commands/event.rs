use tauri::Emitter;

#[derive(serde::Serialize, Clone)]
pub struct DownloadResult {
    pub success: bool,
    pub message: String,
}

pub fn emit_status(window: &tauri::WebviewWindow, ok: bool, msg: impl Into<String>) {
    let _ = window.emit(
        "download-status",
        DownloadResult {
            success: ok,
            message: msg.into(),
        },
    );
}
