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
    let _ = crate::settings::load_settings();
    tauri::Builder::default()
        .manage(DownloadState(Default::default()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            // SETTINGS
            commands::settings_cmd::load_settings,
            commands::settings_cmd::save_settings,

            // HOME / DOWNLOAD
            commands::downloader::download_url,
            commands::downloader::cancel_download,

            // FILES / IMPORT
            commands::files::pick_csv_and_read,
            commands::files::read_csv_from_path,
            commands::files::pick_directory,
            commands::files::open_directory,
            commands::import::import_csv_to_db,

            // LIBRARY / LIST
            commands::list::list_backlog,
            commands::list::list_queue,
            commands::list::list_done,

            // STATUS MUTATIONS
            commands::list::move_link_to_queue,
            commands::list::move_collection_to_queue,
            commands::list::move_platform_to_queue,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
