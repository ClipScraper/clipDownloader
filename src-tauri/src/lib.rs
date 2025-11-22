mod commands;
mod database;
mod download;
mod logging;
mod settings;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let s = crate::settings::load_settings();
    crate::logging::init(s.debug_logs);
    tracing::info!("App starting; debug_logs={}", s.debug_logs);

    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
    let download_manager = crate::download::manager::DownloadManager::new(cmd_tx.clone());

    tauri::Builder::default()
        .manage(download_manager)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard::init())
        .setup(move |app| {
            let app_handle = app.handle();
            let tx_clone = cmd_tx.clone();
            tauri::async_runtime::spawn(crate::download::manager::run_download_manager(
                app_handle.clone(),
                cmd_rx,
                tx_clone,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // SETTINGS
            commands::settings_cmd::load_settings,
            commands::settings_cmd::save_settings,
            // HOME / DOWNLOAD
            commands::downloader::download_url,
            commands::downloader::cancel_download,
            commands::downloader::enqueue_downloads,
            commands::downloader::move_downloads_to_backlog,
            commands::downloader::set_download_paused,
            commands::downloader::refresh_download_settings,
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
            commands::list::list_downloads,
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
