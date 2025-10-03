use std::fs as std_fs;

// [BACKEND] [commands/files.rs] [pick_csv_and_read]
// Opens a file picker dialog for CSV files and reads the selected file
// This is the backend function called when user clicks "Import list" button
// Returns the CSV content as a string for further processing
#[tauri::command]
pub async fn pick_csv_and_read(app: tauri::AppHandle) -> Result<String, String> {
    println!("[BACKEND] [commands/files.rs] [pick_csv_and_read]");
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let mut file_builder = app.dialog().file();
    // Set default directory to user's home folder
    if let Some(home) = dirs::home_dir() {
        file_builder = file_builder.set_directory(home);
    }
    // Configure dialog to only show CSV files
    let picked = file_builder
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();

    let Some(file_path) = picked else { return Err("No file selected".into()) };
    match file_path {
        FilePath::Path(path_buf) => std_fs::read_to_string(path_buf).map_err(|e| e.to_string()),
        FilePath::Url(url) => Err(format!("Unsupported URL selection: {url}")),
    }
}

// [BACKEND] [commands/files.rs] [read_csv_from_path]
// Reads a CSV file from a given file system path
// Used by drag-and-drop functionality to read dropped CSV files
// Returns the CSV content as a string for import processing
#[tauri::command]
pub async fn read_csv_from_path(path: String) -> Result<String, String> {
    println!("[BACKEND] [commands/files.rs] [read_csv_from_path] {}", path);
    println!("[tauri] read_csv_from_path: {}", path);
    std_fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_directory(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();

    if let Some(folder_path) = picked {
        let path_str = match folder_path {
            tauri_plugin_dialog::FilePath::Path(buf) => buf.to_string_lossy().to_string(),
            tauri_plugin_dialog::FilePath::Url(url) => {
                url.to_file_path().unwrap().to_string_lossy().to_string()
            }
        };
        Ok(path_str)
    } else {
        Err("No directory selected".into())
    }
}

#[tauri::command]
pub async fn open_directory(path: String) -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
