use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::database::OnDuplicate;
use crate::commands::parse::{tiktok_id_from_url, last_segment};

fn common_ytdlp_args(cookie_arg: &str, is_ig_images: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "-N".into(), "8".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "--ignore-config".into(),
        "--no-cache-dir".into(),
        "--print".into(), "after_move:filepath".into(),
        "--print".into(), "filepath".into(),
        "--print".into(), "filename".into(),
    ];
    if is_ig_images {
        args.push("--ignore-no-formats-error".into());
    } else {
        args.extend(vec![
            "-f".into(), "bestvideo+bestaudio/best".into(),
            "--merge-output-format".into(), "mp4".into(),
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
    last_segment(url).unwrap_or_else(|| "media".into())
}

fn sanitize<S: Into<String>>(s: S) -> String {
    s.into()
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string()
}

// ADD near the top of video.rs (below helpers), or anywhere above choose_output_template
fn probe_uploader(cookie_arg: &str, processed_url: &str, is_ig_images: bool) -> Option<String> {
    let mut args = common_ytdlp_args(cookie_arg, is_ig_images);
    args.push("--simulate".into());
    args.extend(vec![
        "--print".into(), "uploader".into(),
        processed_url.into(),
    ]);
    match Command::new("yt-dlp").args(&args).output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() { Some(s) } else { None }
        }
        _ => None,
    }
}

/* ---------- probing for non-IG/TT platforms ---------- */

fn probe_filename(cookie_arg: &str, processed_url: &str, is_ig_images: bool) -> std::io::Result<String> {
    let mut args = common_ytdlp_args(cookie_arg, is_ig_images);
    args.push("--simulate".into());
    args.extend(vec![
        "--print".into(), "filename".into(),
        "-o".into(), "%(uploader)s - %(title)s [%(id)s].%(ext)s".into(),
        processed_url.into(),
    ]);

    let out = Command::new("yt-dlp").args(&args).output()?;
    if !out.status.success() {
        let mut args_old = common_ytdlp_args(cookie_arg, is_ig_images);
        args_old.push("--simulate".into());
        args_old.extend(vec![
            "--get-filename".into(),
            "-o".into(), "%(uploader)s - %(title)s [%(id)s].%(ext)s".into(),
            processed_url.into(),
        ]);
        let out_old = Command::new("yt-dlp").args(&args_old).output()?;
        if !out_old.status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("yt-dlp failed to probe filename: {}", String::from_utf8_lossy(&out_old.stderr)),
            ));
        }
        Ok(String::from_utf8_lossy(&out_old.stdout).trim().to_string())
    } else {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/* ---------- output template selection ---------- */

fn choose_output_template(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
    on_duplicate: &OnDuplicate,
) -> std::io::Result<String> {
    let is_instagram = processed_url.contains("instagram.com/");
    let is_tiktok = processed_url.contains("tiktok.com/");

    // 1) Compute the “rest of url” token (e.g. IG id or TT id)
    let rest = sanitize(rest_token_from_url(processed_url));

    // 2) Get the REAL author/uploader first; fallback to URL parsing
    let author_real = probe_uploader(cookie_arg, processed_url, is_ig_images)
        .or_else(|| {
            if is_instagram {
                ig_handle_from_url(processed_url)
            } else if is_tiktok {
                tiktok_username_from_url(processed_url)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into());

    let base_stem = sanitize(format!("{author_real} - {rest}"));

    // We merge to MP4 for videos, so extension is stable.
    let ext = "mp4";
    let mut chosen_stem = base_stem.clone();
    let mut chosen_path = yt_out_dir.join(format!("{chosen_stem}.{ext}"));

    match on_duplicate {
        OnDuplicate::CreateNew => {
            let mut n: usize = 1;
            while chosen_path.exists() {
                n += 1;
                chosen_stem = format!("{base_stem} ({n})");
                chosen_path = yt_out_dir.join(format!("{chosen_stem}.{ext}"));
            }
            println!(
                "[YT-DLP][template] policy=CreateNew dir='{}' author='{}' rest='{}' -> chosen='{}'",
                yt_out_dir.display(), author_real, rest, chosen_path.display()
            );
            // Return a literal template (only %(ext)s is dynamic, but we know it's mp4)
            Ok(format!("{chosen_stem}.%(ext)s"))
        }
        OnDuplicate::Overwrite => {
            let existed = chosen_path.exists();
            println!(
                "[YT-DLP][template] policy=Overwrite dir='{}' author='{}' rest='{}' -> {} '{}'",
                yt_out_dir.display(), author_real, rest,
                if existed { "will overwrite" } else { "will create" },
                chosen_path.display()
            );
            Ok(format!("{base_stem}.%(ext)s"))
        }
        OnDuplicate::DoNothing => {
            let existed = chosen_path.exists();
            println!(
                "[YT-DLP][template] policy=DoNothing dir='{}' author='{}' rest='{}' -> {} '{}'",
                yt_out_dir.display(), author_real, rest,
                if existed { "exists (will skip)" } else { "will create" },
                chosen_path.display()
            );
            Ok(format!("{base_stem}.%(ext)s"))
        }
    }
}

/* ---------- runner ---------- */

pub fn run_yt_dlp_with_progress<F>(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
    on_duplicate: &OnDuplicate,
    mut progress_callback: F,
) -> std::io::Result<(bool, String)>
where
    F: FnMut(&str),
{
    let mut args = common_ytdlp_args(cookie_arg, is_ig_images);

    // Respect Settings: Overwrite / CreateNew / DoNothing
    args.extend(crate::settings::get_yt_dlp_duplicate_flags(on_duplicate));

    // Force destination directory ⇒ never spill into repo
    args.push("-P".into());
    args.push(yt_out_dir.to_string_lossy().to_string());

    // Our "{author} - {rest}" output pattern (with uniqueness if needed)
    let output_template = choose_output_template(
        yt_out_dir, cookie_arg, processed_url, is_ig_images, on_duplicate,
    )?;
    args.push("-o".into());
    args.push(output_template);

    // URL last
    args.push(processed_url.to_string());

    let mut child = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let mut all_output = String::new();
    let mut already_downloaded = false;
    let mut file_skipped = false;

    for line in stdout_reader.lines() {
        if let Ok(line) = line {
            all_output.push_str(&line);
            all_output.push('\n');

            if line.contains("has already been downloaded") {
                already_downloaded = true;
            }
            if line.contains("has already been recorded in the archive")
                || line.contains("[download] Skipping")
            {
                file_skipped = true;
            }

            if (line.contains("[download]") || line.contains("[info]"))
                && !line.contains("Starting download for")
                && !line.contains("Sleeping")
                && !line.starts_with("[info] Downloading")
            {
                progress_callback(&line);
            }
        }
    }

    for line in stderr_reader.lines() {
        if let Ok(line) = line {
            all_output.push_str(&line);
            all_output.push('\n');
        }
    }

    let status = child.wait()?;
    let ok = status.success() || already_downloaded || file_skipped;
    Ok((ok, all_output))
}
