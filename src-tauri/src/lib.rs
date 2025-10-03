mod commands;
mod database;
mod download;
mod settings;
mod utils;

use std::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct DownloadState(pub std::sync::Arc<Mutex<Option<JoinHandle<()>>>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DownloadState(Default::default()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::download_url,
            commands::load_settings,
            commands::save_settings,
            commands::pick_csv_and_read,
            commands::read_csv_from_path,
            commands::pick_directory,
            commands::open_directory,
            commands::cancel_download,
            commands::import_csv_to_db,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
