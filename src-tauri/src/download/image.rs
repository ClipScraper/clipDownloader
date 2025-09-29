use std::path::Path;
use std::process::Command;

// Try several ways to invoke gallery-dl (Homebrew path, /usr/local, PATH, and python -m)
pub fn gallery_dl_candidates() -> Vec<(String, Vec<String>)> {
    vec![
        ("/opt/homebrew/bin/gallery-dl".into(), vec![]),
        ("/usr/local/bin/gallery-dl".into(), vec![]),
        ("gallery-dl".into(), vec![]),
        ("python3".into(), vec!["-m".into(), "gallery_dl".into()]),
    ]
}

// For gallery-dl we pass the *root* download dir (user setting) as base directory.
// That way gallery-dl creates "instagram/<user>/..." or "tiktok/<user>/..." under it,
// avoiding the duplicate "<platform>/<platform>/..." you saw.
pub fn run_gallery_dl(base_download_dir: &Path, url: &str, cookie_arg: &str) -> std::io::Result<std::process::Output> {
    // NOTE: Do NOT pass "--progress" (not supported in your version).
    // "-d" is an alias for "-o base-directory=..."
    let base_args = vec![
        "--verbose".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "-d".into(), base_download_dir.display().to_string(),
        url.into(),
    ];

    let mut last_err: Option<std::io::Error> = None;
    for (prog, prefix) in gallery_dl_candidates() {
        let mut args = prefix.clone();
        args.extend(base_args.clone());

        match Command::new(&prog).args(&args).output() {
            Ok(out) => return Ok(out),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "gallery-dl not found")))
}
