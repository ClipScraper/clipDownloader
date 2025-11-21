use std::io;
use std::path::PathBuf;
use tempfile::tempdir;
use tauri::Manager;
use tauri_plugin_shell::{process::{CommandEvent, CommandChild}, ShellExt};
use tokio::time::{timeout, Duration};

struct KillGuard(Option<CommandChild>);
impl Drop for KillGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.take() {
            let _ = c.kill();
        }
    }
}

#[cfg(target_family = "windows")]
fn path_sep() -> &'static str { ";" }
#[cfg(not(target_family = "windows"))]
fn path_sep() -> &'static str { ":" }

/// Run gallery-dl (sidecar) into a temp dir; return (ok, output, tmp_path).
pub async fn run_gallery_dl_to_temp(app: &tauri::AppHandle, _base_download_dir: &std::path::Path, url: &str, cookie_arg: &str, window: &tauri::WebviewWindow) -> io::Result<(bool, String, PathBuf)> {
    let tmp = tempdir()?;
    #[allow(deprecated)]
    let tmp_path = tmp.into_path(); // keep the directory; caller cleans up

    let res_dir = app.path().resource_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let new_path = format!("{}{}{}", res_dir.to_string_lossy(), path_sep(), std::env::var("PATH").unwrap_or_default());

    let args = vec![
        "--verbose".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "-d".into(), tmp_path.display().to_string(),
        url.into(),
    ];

    let cmd = app.shell().sidecar("gallery-dl")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("sidecar(gallery-dl) error: {e}")))?;

    let (mut rx, child) = cmd.args(args)
        .env("PATH", new_path)
        .spawn()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("spawn gallery-dl failed: {e}")))?;

    let _guard = KillGuard(Some(child));

    let mut all_output = String::new();
    let mut ok = false;

    loop {
        let ev = match timeout(Duration::from_secs(180), rx.recv()).await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => {
                eprintln!("[tauri] gallery-dl timed out (no output for 180s)");
                return Err(io::Error::new(io::ErrorKind::TimedOut, "gallery-dl timed out"));
            }
        };

        match ev {
            CommandEvent::Stdout(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                for line in s.lines() {
                    let l = line.trim();
                    if !l.is_empty() {
                        all_output.push_str(l);
                        all_output.push('\n');
                        crate::commands::event::emit_status(window, url, true, l.to_string());
                    }
                }
            }
            CommandEvent::Stderr(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                all_output.push_str(&s);
            }
            CommandEvent::Terminated(code) => {
                ok = code.code == Some(0);
            }
            _ => {}
        }
    }

    Ok((ok, all_output, tmp_path))
}
