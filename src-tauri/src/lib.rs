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
            commands::downloader::download_url,

            // SETTINGS
            commands::settings_cmd::load_settings,
            commands::settings_cmd::save_settings,

            // HOME
            commands::files::pick_csv_and_read,
            commands::files::read_csv_from_path,
            commands::files::pick_directory,
            commands::files::open_directory,
            commands::downloader::cancel_download,
            commands::import::import_csv_to_db,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
