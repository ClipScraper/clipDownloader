mod commands;
mod download;
mod settings;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::download_url,
            commands::load_settings,
            commands::save_settings,
            commands::pick_csv_and_read,
            commands::read_csv_from_path,
            commands::pick_directory,
            commands::open_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
