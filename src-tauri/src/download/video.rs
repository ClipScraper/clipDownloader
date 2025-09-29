use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run_yt_dlp_with_progress<F>(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig: bool,
    mut progress_callback: F,
) -> std::io::Result<(bool, String)>
where
    F: FnMut(&str),
{
    let mut args: Vec<String> = vec![
        "--newline".into(), // Force newlines for progress updates
        "-N".into(),
        "8".into(),
        "--cookies-from-browser".into(),
        (*cookie_arg).into(),
        "--ignore-config".into(),
        "--no-cache-dir".into(),
    ];

    let template = if is_ig {
        // IG posts: allow autonumber for carousels; don't hard fail if some items are images
        args.push("--ignore-no-formats-error".into());
        format!(
            "{}/%(uploader)s - %(title)s [%(id)s]-%(autonumber)03d.%(ext)s",
            yt_out_dir.display()
        )
    } else {
        // Video sites (TikTok *video*, YouTube...)
        args.extend(vec![
            "-f".into(),
            "bestvideo+bestaudio/best".into(),
            "--merge-output-format".into(),
            "mp4".into(),
        ]);
        format!(
            "{}/%(uploader)s - %(title)s [%(id)s].%(ext)s",
            yt_out_dir.display()
        )
    };

    args.push("-o".into());
    args.push(template);
    args.push(processed_url.to_string());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let mut child = Command::new("yt-dlp")
        .args(&arg_refs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let mut all_output = String::new();
    let mut already_downloaded = false;

    // Read stdout for progress
    for line in stdout_reader.lines() {
        if let Ok(line) = line {
            all_output.push_str(&line);
            all_output.push('\n');
            
            if line.contains("has already been downloaded") {
                already_downloaded = true;
            }
            
            // Pass progress lines to callback
            if line.contains("[download]") || line.contains("[info]") {
                progress_callback(&line);
            }
        }
    }

    // Read stderr (yt-dlp outputs some info to stderr)
    for line in stderr_reader.lines() {
        if let Ok(line) = line {
            all_output.push_str(&line);
            all_output.push('\n');
        }
    }

    let status = child.wait()?;
    Ok((status.success() || already_downloaded, all_output))
}
