use std::fs as std_fs;

#[tauri::command]
pub async fn pick_csv_and_read(app: tauri::AppHandle) -> Result<String, String> {
    println!("[BACKEND] [commands/files.rs] [pick_csv_and_read]");
    use tauri_plugin_dialog::{DialogExt, FilePath};

    let mut file_builder = app.dialog().file();
    if let Some(home) = dirs::home_dir() {
        file_builder = file_builder.set_directory(home);
    }

    let picked = file_builder
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();

    let Some(file_path) = picked else {
        return Err("No file selected".into());
    };

    match file_path {
        FilePath::Path(path_buf) => {
            let csv_text = std_fs::read_to_string(path_buf).map_err(|e| e.to_string())?;

            match super::import::import_csv_text(csv_text.clone()).await {
                Ok(n) => println!("[BACKEND] [files] imported {n} rows (picker)"),
                Err(e) => {
                    eprintln!("[BACKEND] [files] import failed: {e}");
                    return Err(e);
                }
            }

            Ok(csv_text)
        }
        FilePath::Url(url) => Err(format!("Unsupported URL selection: {url}")),
    }
}

#[tauri::command]
pub async fn read_csv_from_path(path: String) -> Result<String, String> {
    println!(
        "[BACKEND] [commands/files.rs] [read_csv_from_path] {}",
        path
    );

    let csv_text = std_fs::read_to_string(&path).map_err(|e| e.to_string())?;

    match super::import::import_csv_text(csv_text.clone()).await {
        Ok(n) => println!(
            "[BACKEND] [files] imported {n} rows (drag-drop) from {}",
            path
        ),
        Err(e) => {
            eprintln!("[BACKEND] [files] import failed for {path}: {e}");
            return Err(e);
        }
    }
    Ok(csv_text)
}

#[tauri::command]
pub async fn pick_directory(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath}; // ← bring FilePath into scope

    let mut builder = app.dialog().file();
    if let Some(home) = dirs::home_dir() {
        builder = builder.set_directory(home);
    }

    let picked = builder.blocking_pick_folder();
    match picked {
        Some(FilePath::Path(path)) => Ok(path.display().to_string()),
        Some(FilePath::Url(url)) => Err(format!("Unsupported URL folder: {url}")),
        None => Err("No folder selected".into()),
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
