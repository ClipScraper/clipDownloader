use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::settings::OnDuplicate;

/// Build the common yt-dlp args (cookies, parallel, etc.), and the
/// format args depending on IG vs video sites.
fn common_ytdlp_args(cookie_arg: &str, is_ig: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),                   // progress lines
        "-N".into(), "8".into(),              // parallel fragments
        "--cookies-from-browser".into(), cookie_arg.into(),
        "--ignore-config".into(),
        "--no-cache-dir".into(),
    ];
    if is_ig {
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

/// Ask yt-dlp what the *final* filename would be for our base template.
/// Returns just the "name.ext" (no directories).
fn probe_filename(cookie_arg: &str, processed_url: &str, is_ig: bool) -> std::io::Result<String> {
    let mut args = common_ytdlp_args(cookie_arg, is_ig);

    // We want the evaluated filename for this template (no directories here).
    args.extend(vec![
        "--print".into(), "filename".into(),
        "-o".into(), "%(uploader)s - %(title)s [%(id)s].%(ext)s".into(),
        processed_url.into(),
    ]);

    let out = Command::new("yt-dlp").args(&args).output()?;
    if !out.status.success() {
        // Fall back to the older --get-filename if needed
        let mut args_old = common_ytdlp_args(cookie_arg, is_ig);
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

/// If policy is CreateNew, compute a unique "-o" value like:
///   <dir>/<stem>.%(ext)s
/// or  <dir>/<stem> (2).%(ext)s, <dir>/<stem> (3).%(ext)s, ...
fn choose_output_template(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig: bool,
    on_duplicate: &OnDuplicate,
) -> std::io::Result<String> {
    // Base template (without directories) for the name we want:
    // "%(uploader)s - %(title)s [%(id)s].%(ext)s"
    // We attach the directory ourselves below.
    let base_template = "%(uploader)s - %(title)s [%(id)s]";

    match on_duplicate {
        OnDuplicate::CreateNew => {
            // Ask yt-dlp what the resolved filename would be with our format args.
            // e.g., "uploader - title [id].mp4"
            let base_filename = probe_filename(cookie_arg, processed_url, is_ig)?;
            // Extract stem + extension from the probed filename
            let p = PathBuf::from(base_filename);
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("mp4"); // safe default for video sites
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("video");

            // If "<dir>/<stem>.<ext>" already exists, use "<dir>/<stem> (n).<ext>".
            let mut n: usize = 1;
            let mut candidate = yt_out_dir.join(format!("{stem}.{ext}"));
            while candidate.exists() {
                n += 1;
                candidate = yt_out_dir.join(format!("{stem} ({n}).{ext}"));
            }

            // Build an -o with "%(ext)s" so yt-dlp can still choose the final codec/ext,
            // but suffix matches the free slot we found.
            if n == 1 {
                Ok(format!("{}/{}.%(ext)s", yt_out_dir.display(), stem))
            } else {
                Ok(format!("{}/{} ({}).%(ext)s", yt_out_dir.display(), stem, n))
            }
        }
        _ => {
            // Overwrite / DoNothing → use the base template unchanged
            Ok(format!("{}/{}.%({})s", yt_out_dir.display(), base_template, "ext")
                .replace("%(ext)s", "%(ext)s"))
        }
    }
}

pub fn run_yt_dlp_with_progress<F>(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig: bool,
    on_duplicate: &OnDuplicate,
    mut progress_callback: F,
) -> std::io::Result<(bool, String)>
where
    F: FnMut(&str),
{
    // Start with the common args
    let mut args = common_ytdlp_args(cookie_arg, is_ig);

    // Add duplicate-handling flags
    args.extend(crate::settings::get_yt_dlp_duplicate_flags(on_duplicate));

    // Decide the output template (and ensure uniqueness for CreateNew)
    let output_template = choose_output_template(yt_out_dir, cookie_arg, processed_url, is_ig, on_duplicate)?;
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
    // (message text is decided by the caller using the policy)
    let ok = status.success() || already_downloaded || file_skipped;
    Ok((ok, all_output))
}
