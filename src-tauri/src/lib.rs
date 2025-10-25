mod commands;
mod database;
mod download;
mod settings;
mod utils;
mod logging;

use std::sync::Mutex;
use tokio::task::JoinHandle;
use std::collections::HashMap;

#[derive(Clone)]
pub struct DownloadState(pub std::sync::Arc<Mutex<HashMap<String, JoinHandle<()>>>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let s = crate::settings::load_settings();
    crate::logging::init(s.debug_logs);
    tracing::info!("App starting; debug_logs={}", s.debug_logs);

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
            commands::list::toggle_output_format,
            commands::list::set_output_format,

            // STATUS MUTATIONS
            commands::list::move_link_to_queue,
            commands::list::move_collection_to_queue,
            commands::list::move_platform_to_queue,
            commands::list::move_link_to_backlog,
            commands::list::move_collection_to_backlog,
            commands::list::move_platform_to_backlog,
            commands::list::delete_rows_by_platform,
            commands::list::delete_rows_by_collection,

            // FRONTEND LOGGING
            commands::log::frontend_log,

            // NEW: Library item actions
            commands::library::open_file_for_link,
            commands::library::open_folder_for_link,
            commands::library::open_platform_folder,
            commands::library::open_collection_folder,
            commands::library::delete_library_item,
            commands::list::delete_rows_by_link,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
