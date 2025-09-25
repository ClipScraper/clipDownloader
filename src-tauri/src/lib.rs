// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn download_url(url: String) -> Result<String, String> {
    use std::path::PathBuf;
    use std::process::Command;

    let base = PathBuf::from("/Users/hjoncour/Downloads/dld");
    let site = if url.contains("instagram.com") {
        "instagram"
    } else if url.contains("tiktok.com") {
        "tiktok"
    } else if url.contains("youtube.com") || url.contains("youtu.be") {
        "youtube"
    } else {
        "other"
    };

    let out_dir = base.join(site);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let template = format!("{}/%(uploader)s - %(title)s [%(id)s].%(ext)s", out_dir.display());
    let args = vec![
        "-N", "8",
        "-f", "bestvideo+bestaudio/best",
        "--merge-output-format", "mp4",
        "-o", &template,
        &url,
    ];

    let status = Command::new("yt-dlp").args(&args).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(format!("Saved to {}", out_dir.display())) } else { Err(format!("yt-dlp exited with status {status}")) }
}

#[tauri::command]
async fn pick_csv_and_read(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let mut file_builder = app.dialog().file();
    // Start in the user's home directory when possible
    if let Some(home) = dirs::home_dir() {
        file_builder = file_builder.set_directory(home);
    }
    // Filters are optional; accept csv and text
    let picked = file_builder
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();

    let Some(file_path) = picked else { return Err("No file selected".into()) };
    match file_path {
        FilePath::Path(path_buf) => std::fs::read_to_string(path_buf).map_err(|e| e.to_string()),
        FilePath::Url(url) => Err(format!("Unsupported URL selection: {url}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![greet, pick_csv_and_read, download_url])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
