use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::database::OnDuplicate;

/// Build the common yt-dlp args (cookies, parallel, etc.), and the
/// format args depending on IG vs video sites.
fn common_ytdlp_args(cookie_arg: &str, is_ig_images: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),                   // progress lines
        "-N".into(), "8".into(),              // parallel fragments
        "--cookies-from-browser".into(), cookie_arg.into(),
        "--ignore-config".into(),
        "--no-cache-dir".into(),
        // Ask yt-dlp to print the final saved path even after post-processing
        "--print".into(), "after_move:filepath".into(),
        // Also print generic planned filepath/filename as fallbacks (useful on skips)
        "--print".into(), "filepath".into(),
        "--print".into(), "filename".into(),
    ];
    if is_ig_images {
        // Allow IG posts without video formats (carousels with images)
        args.push("--ignore-no-formats-error".into());
    } else {
        // Prefer best video + audio, merge to mp4
        args.extend(vec![
            "-f".into(), "bestvideo+bestaudio/best".into(),
            "--merge-output-format".into(), "mp4".into(),
        ]);
    }
    args
}

/// Quick IG id extraction from URL (/reel/:id or /p/:id)
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

/// Ask yt-dlp what the *final* filename would be for our base template.
/// Returns just the "name.ext" (no directories).
fn probe_filename(cookie_arg: &str, processed_url: &str, is_ig_images: bool) -> std::io::Result<String> {
    let mut args = common_ytdlp_args(cookie_arg, is_ig_images);

    // We want the evaluated filename for this template (no directories here).
    args.extend(vec![
        "--print".into(), "filename".into(),
        "-o".into(), "%(uploader)s - %(title)s [%(id)s].%(ext)s".into(),
        processed_url.into(),
    ]);

    let out = Command::new("yt-dlp").args(&args).output()?;
    if !out.status.success() {
        // Fall back to the older --get-filename if needed
        let mut args_old = common_ytdlp_args(cookie_arg, is_ig_images);
        args_old.extend(vec![
            "--get-filename".into(),
            "-o".into(), "%(uploader)s - %(title)s [%(id)s].%(ext)s".into(),
            processed_url.into(),
        ]);
        let out_old = Command::new("yt-dlp").args(&args_old).output()?;
        if !out_old.status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "yt-dlp failed to probe filename: {}",
                    String::from_utf8_lossy(&out_old.stderr)
                ),
            ));
        }
        Ok(String::from_utf8_lossy(&out_old.stdout).trim().to_string())
    } else {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/// If policy is CreateNew, compute a unique output *pattern* (no directory here).
/// We pass the directory separately via `-P <dir>` to guarantee location.
fn choose_output_template(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig_images: bool,
    on_duplicate: &OnDuplicate,
) -> std::io::Result<String> {
    // For IG reels and posts handled by yt-dlp, we want filename = "%(id)s"
    let id_only = processed_url.contains("instagram.com/")
        && (processed_url.contains("/reel/") || processed_url.contains("/p/"));

    match on_duplicate {
        OnDuplicate::CreateNew => {
            // Decide the stem to probe uniqueness for:
            // - IG id-only: use the token from URL and assume mp4 for existence checks
            // - Others: probe yt-dlp to get a realistic ext
            if id_only {
                let stem = ig_id_from_url(processed_url).unwrap_or_else(|| "video".into());
                let ext = "mp4"; // practical default for reels
                let mut n: usize = 1;
                let mut candidate = yt_out_dir.join(format!("{stem}.{ext}"));
                while candidate.exists() {
                    n += 1;
                    candidate = yt_out_dir.join(format!("{stem} ({n}).{ext}"));
                }
                if n == 1 {
                    Ok(format!("{}.%(ext)s", stem))
                } else {
                    Ok(format!("{} ({}).%(ext)s", stem, n))
                }
            } else {
                let base_filename = probe_filename(cookie_arg, processed_url, is_ig_images)?;
                let p = PathBuf::from(base_filename);
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
                let mut n: usize = 1;
                let mut candidate = yt_out_dir.join(format!("{stem}.{ext}"));
                while candidate.exists() {
                    n += 1;
                    candidate = yt_out_dir.join(format!("{stem} ({n}).{ext}"));
                }
                if n == 1 {
                    Ok(format!("{}.%(ext)s", stem))
                } else {
                    Ok(format!("{} ({}).%(ext)s", stem, n))
                }
            }
        }
        _ => {
            // Overwrite / DoNothing → use a stable template (id for IG reels/posts)
            if id_only {
                Ok("%(id)s.%(ext)s".into())
            } else {
                Ok("%(uploader)s - %(title)s [%(id)s].%(ext)s".into())
            }
        }
    }
}

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
    // Start with the common args
    let mut args = common_ytdlp_args(cookie_arg, is_ig_images);

    // Add duplicate-handling flags
    args.extend(crate::settings::get_yt_dlp_duplicate_flags(on_duplicate));

    // Guarantee the download directory: never spill into repo folder
    args.push("-P".into());
    args.push(yt_out_dir.to_string_lossy().to_string());

    // Decide the output template (and ensure uniqueness for CreateNew)
    let output_template = choose_output_template(yt_out_dir, cookie_arg, processed_url, is_ig_images, on_duplicate)?;
    args.push("-o".into());
    args.push(output_template);

    // Finally, add the URL
    args.push(processed_url.to_string());

    // Run yt-dlp and stream progress
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

            // Only show actual download progress, not initial messages
            if (line.contains("[download]") || line.contains("[info]"))
                && !line.contains("Starting download for")
                && !line.contains("Sleeping")
                && !line.starts_with("[info] Downloading") {
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

    // Success if yt-dlp succeeded OR it reported "already downloaded/skipped".
    let ok = status.success() || already_downloaded || file_skipped;
    Ok((ok, all_output))
}
