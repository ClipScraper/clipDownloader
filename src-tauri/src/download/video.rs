use std::io;
use std::path::Path;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

use crate::commands::parse::{last_segment, tiktok_id_from_url, youtube_id_from_url};
use crate::database::OnDuplicate;
use crate::download::manager::DownloadEvent;

use tauri::Manager;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};

struct KillGuard(Option<CommandChild>);
impl Drop for KillGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.take() {
            let _ = c.kill();
        }
    }
}

#[cfg(target_family = "windows")]
fn path_sep() -> &'static str {
    ";"
}
#[cfg(not(target_family = "windows"))]
fn path_sep() -> &'static str {
    ":"
}

fn base_ytdlp_args(cookie_arg: &str, is_ig_images: bool, audio_only: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "-N".into(),
        "8".into(),
        "--cookies-from-browser".into(),
        cookie_arg.into(),
        "--ignore-config".into(),
        "--no-cache-dir".into(),
    ];
    if is_ig_images {
        args.push("--ignore-no-formats-error".into());
    } else if audio_only {
        args.extend(vec![
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--audio-quality".into(),
            "0".into(),
        ]);
    } else {
        args.extend(vec![
            "-f".into(),
            "bestvideo+bestaudio/best".into(),
            "--merge-output-format".into(),
            "mp4".into(),
        ]);
    }
    args
}

/* ---------- helpers to read parts from the URL ---------- */
fn ig_id_from_url(url: &str) -> Option<String> {
    if let Some(pos) = url.find("instagram.com/") {
        let rest = &url[pos + "instagram.com/".len()..];
        let parts: Vec<&str> = rest.trim_matches('/').split('/').collect();
        if parts.len() >= 3 {
            let typ = parts[1];
            if typ == "reel" || typ == "p" {
                return Some(parts[2].to_string());
            }
        }
    }
    None
}

fn ig_handle_from_url(url: &str) -> Option<String> {
    if let Some(pos) = url.find("instagram.com/") {
        let first = &url[pos + "instagram.com/".len()..];
        let seg = first.trim_matches('/').split('/').next().unwrap_or("");
        if !seg.is_empty() && seg != "reel" && seg != "p" {
            return Some(seg.to_string());
        }
    }
    None
}

fn tiktok_username_from_url(url: &str) -> Option<String> {
    if let Some(idx) = url.find("tiktok.com/@") {
        let tail = &url[idx + "tiktok.com/@".len()..];
        let handle = tail.split(['/', '?', '&']).next().unwrap_or("");
        if !handle.is_empty() {
            return Some(handle.to_string());
        }
    }
    None
}

/// the “rest-of-url” token:
/// - IG: id after /reel/ or /p/, else last path segment
/// - TikTok: id after /video/ or /photo/, else last path segment
/// - YouTube: v=… or /shorts/…
fn rest_token_from_url(url: &str) -> String {
    if url.contains("instagram.com/") {
        if let Some(id) = ig_id_from_url(url) {
            return id;
        }
    }
    if url.contains("tiktok.com/") {
        if let Some(id) = tiktok_id_from_url(url) {
            return id;
        }
    }
    if url.contains("youtube.com/") || url.contains("youtu.be/") {
        if let Some(id) = youtube_id_from_url(url) {
            return id;
        }
    }
    last_segment(url).unwrap_or_else(|| "media".into())
}

fn sanitize<S: Into<String>>(s: S) -> String {
    let t = s
        .into()
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .replace(['\n', '\r', '\t'], " ");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

// Probe uploader using yt-dlp sidecar (simulate + print)
async fn probe_uploader(
    app: &tauri::AppHandle,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
) -> Option<String> {
    let mut args = base_ytdlp_args(cookie_arg, is_ig_images, false);
    args.push("--simulate".into());
    args.extend(vec![
        "--print".into(),
        "uploader".into(),
        processed_url.into(),
    ]);

    let settings = crate::settings::load_settings();
    let cmd = if settings.use_system_binaries {
        app.shell().command("yt-dlp")
    } else {
        match app.shell().sidecar("yt-dlp") {
            Ok(c) => c,
            Err(_) => return None,
        }
    };

    // Ensure PATH contains the resources dir so ffmpeg is discoverable if needed
    use tauri::path::BaseDirectory;
    let res_dir = app
        .path()
        .resolve("", BaseDirectory::Resource)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let new_path = if settings.use_system_binaries {
        std::env::var("PATH").unwrap_or_default()
    } else {
        format!(
            "{}{}{}",
            res_dir.to_string_lossy(),
            path_sep(),
            std::env::var("PATH").unwrap_or_default()
        )
    };

    let Ok((mut rx, _child)) = cmd.args(args).env("PATH", new_path).spawn() else {
        return None;
    };

    let mut first_line: Option<String> = None;
    while let Some(ev) = rx.recv().await {
        if let CommandEvent::Stdout(bytes) = ev {
            let s = String::from_utf8_lossy(&bytes);
            for line in s.lines() {
                let l = line.trim();
                if !l.is_empty() && first_line.is_none() {
                    first_line = Some(l.to_string());
                }
            }
        }
    }

    first_line
        .filter(|s| !s.eq_ignore_ascii_case("na") && !s.eq_ignore_ascii_case("n/a"))
        .map(sanitize)
}

/* ---------- output template selection ---------- */

async fn choose_output_template(
    app: &tauri::AppHandle,
    out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
    audio_only: bool,
    on_duplicate: &OnDuplicate,
) -> io::Result<String> {
    let rest_id = sanitize(rest_token_from_url(processed_url));

    let mut author_real = probe_uploader(app, cookie_arg, processed_url, is_ig_images)
        .await
        .or_else(|| {
            if processed_url.contains("instagram.com/") {
                ig_handle_from_url(processed_url)
            } else if processed_url.contains("tiktok.com/") {
                tiktok_username_from_url(processed_url)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into());
    author_real = sanitize(author_real);

    let base_stem = format!("{author_real} [{rest_id}]");
    let ext = if audio_only { "mp3" } else { "mp4" };

    let mut chosen_stem = base_stem.clone();
    let chosen_path = out_dir.join(format!("{chosen_stem}.{ext}"));

    match on_duplicate {
        OnDuplicate::CreateNew => {
            if chosen_path.exists() {
                let mut n: usize = 2;
                loop {
                    let cand = out_dir.join(format!("{base_stem}({n}).{ext}"));
                    if !cand.exists() {
                        chosen_stem = format!("{base_stem}({n})");
                        break;
                    }
                    n += 1;
                }
            }
            Ok(format!("{chosen_stem}.%(ext)s"))
        }
        OnDuplicate::Overwrite => Ok(format!("{base_stem}.%(ext)s")),
        OnDuplicate::DoNothing => Ok(format!("{base_stem}.%(ext)s")),
    }
}

/* ---------- runner ---------- */

pub async fn run_yt_dlp_with_progress(
    app: &tauri::AppHandle,
    out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
    on_duplicate: &OnDuplicate,
    id: i64,
    emitter: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
) -> io::Result<(bool, String)> {
    let audio_only = processed_url.ends_with("#__audio_only__");
    let real_url = if audio_only {
        &processed_url[..processed_url.len() - "#__audio_only__".len()]
    } else {
        processed_url
    };

    let mut args = base_ytdlp_args(cookie_arg, is_ig_images, audio_only);
    args.extend(crate::settings::get_yt_dlp_duplicate_flags(on_duplicate));

    // Prints used by parse_multiple_filenames_from_output
    args.extend(vec![
        "--print".into(),
        "after_move:filepath".into(),
        "--print".into(),
        "filepath".into(),
        "--print".into(),
        "filename".into(),
    ]);

    // Destination directory (avoid spills)
    args.push("-P".into());
    args.push(out_dir.to_string_lossy().to_string());

    // Load settings to determine whether to use system binaries
    let settings = crate::settings::load_settings();

    // Determine resource dir for bundled ffmpeg (when not using system binaries)
    use tauri::path::BaseDirectory;
    let res_dir = app
        .path()
        .resolve("", BaseDirectory::Resource)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    // Only force ffmpeg location when using bundled sidecar tools
    if !settings.use_system_binaries {
        args.push("--ffmpeg-location".into());
        args.push(res_dir.to_string_lossy().to_string());
    }

    // Output template with uniqueness policy
    let output_template = choose_output_template(
        app,
        out_dir,
        cookie_arg,
        real_url,
        is_ig_images,
        audio_only,
        on_duplicate,
    )
    .await?;
    args.push("-o".into());
    args.push(output_template.clone());

    // URL last
    args.push(real_url.to_string());

    let planned_path =
        out_dir.join(output_template.replace("%(ext)s", if audio_only { "mp3" } else { "mp4" }));
    println!(
        "[YT-DLP][sidecar] policy={:?} dir='{}'\nurl='{}'\nout='{}'",
        on_duplicate,
        out_dir.display(),
        real_url,
        planned_path.display()
    );

    let cmd = if settings.use_system_binaries {
        app.shell().command("yt-dlp")
    } else {
        app.shell()
            .sidecar("yt-dlp")
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("sidecar(yt-dlp) error: {e}")))?
    };

    let new_path = if settings.use_system_binaries {
        // Use system PATH as-is to locate system-installed tools
        std::env::var("PATH").unwrap_or_default()
    } else {
        // Prepend bundled resources so yt-dlp can find ffmpeg from the app
        format!(
            "{}{}{}",
            res_dir.to_string_lossy(),
            path_sep(),
            std::env::var("PATH").unwrap_or_default()
        )
    };

    let (mut rx, child) =
        cmd.args(args).env("PATH", new_path).spawn().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("spawn yt-dlp failed: {e}"))
        })?;

    let _guard = KillGuard(Some(child));

    let mut all_output = String::new();
    let mut already_downloaded = false;
    let mut file_skipped = false;
    let mut ok = false;

    loop {
        // Yield to allow other tasks (like event emission) to run
        tokio::task::yield_now().await;

        let event = match timeout(Duration::from_secs(180), rx.recv()).await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => {
                eprintln!("[tauri] yt-dlp timed out (no output for 180s)");
                return Err(io::Error::new(io::ErrorKind::TimedOut, "yt-dlp timed out"));
            }
        };

        match event {
            CommandEvent::Stdout(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                for line in s.lines() {
                    let l = line.trim();
                    all_output.push_str(l);
                    all_output.push('\n');

                    if l.contains("has already been downloaded") {
                        already_downloaded = true;
                    }
                    if l.contains("has already been recorded in the archive")
                        || l.starts_with("[download] Skipping")
                    {
                        file_skipped = true;
                    }

                    if let Some(progress) = parse_progress_percentage(l) {
                        (emitter)(DownloadEvent::Progress {
                            id,
                            progress,
                            downloaded_bytes: 0,
                            total_bytes: None,
                        });
                    } else if (l.contains("[download]") || l.contains("[info]"))
                        && !l.contains("Starting download for")
                        && !l.contains("Sleeping")
                        && !l.starts_with("[info] Downloading")
                    {
                        (emitter)(DownloadEvent::Message {
                            id,
                            message: l.to_string(),
                        });
                    }
                }
            }
            CommandEvent::Stderr(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                for line in s.lines() {
                    let l = line.trim();
                    if !l.is_empty() {
                        all_output.push_str(l);
                        all_output.push('\n');
                        (emitter)(DownloadEvent::Message {
                            id,
                            message: l.to_string(),
                        });
                    }
                }
            }
            CommandEvent::Terminated(code) => {
                ok = code.code == Some(0) || already_downloaded || file_skipped;
            }
            _ => {}
        }
    }

    Ok((ok, all_output))
}

fn parse_progress_percentage(line: &str) -> Option<f32> {
    if !line.contains("[download]") {
        return None;
    }
    let percent_idx = line.find('%')?;
    let start = line[..percent_idx]
        .rsplit_once(' ')
        .map(|(_, tail)| tail)
        .unwrap_or(line[..percent_idx].trim());
    start
        .parse::<f32>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}
