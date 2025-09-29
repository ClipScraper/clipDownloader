use std::path::Path;
use std::process::{Command, Output};

pub fn run_yt_dlp(
    yt_out_dir: &Path,
    cookie_arg: &str,
    processed_url: &str,
    is_ig: bool,
) -> std::io::Result<Output> {
    let mut args: Vec<String> = vec![
        "--verbose".into(),
        "-N".into(),
        "8".into(),
        "--cookies-from-browser".into(),
        (*cookie_arg).into(),
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

    Command::new("yt-dlp").args(&arg_refs).output()
}
