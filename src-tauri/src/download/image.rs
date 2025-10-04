use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Try several ways to invoke gallery-dl (Homebrew path, /usr/local, PATH, and python -m)
pub fn gallery_dl_candidates() -> Vec<(String, Vec<String>)> {
    vec![
        ("/opt/homebrew/bin/gallery-dl".into(), vec![]),
        ("/usr/local/bin/gallery-dl".into(), vec![]),
        ("gallery-dl".into(), vec![]),
        ("python3".into(), vec!["-m".into(), "gallery_dl".into()]),
    ]
}

/// Low-level runner used internally.
/// NOTE: `-d` must point to an existing directory.
fn run_gallery_dl_raw(out_dir: &Path, url: &str, cookie_arg: &str) -> io::Result<std::process::Output> {
    // No progress flag; keep output parseable.
    let base_args = vec![
        "--verbose".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "-d".into(), out_dir.display().to_string(),
        url.into(),
    ];

    let mut last_err: Option<io::Error> = None;
    for (prog, prefix) in gallery_dl_candidates() {
        let mut args = prefix.clone();
        args.extend(base_args.clone());

        match Command::new(&prog).args(&args).output() {
            Ok(out) => return Ok(out),
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "gallery-dl not found")))
}

/// Run gallery-dl into a **temp directory** under the user’s download root.
/// Caller can then move files with a duplicate policy.
pub fn run_gallery_dl_to_temp(_base_download_dir: &std::path::Path, url: &str, cookie_arg: &str) -> std::io::Result<(std::process::Output, PathBuf)> {
    // make a temp dir we will move from afterwards
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.into_path(); // persist for caller

    let base_args = vec![
        "--verbose".into(),
        "--cookies-from-browser".into(), cookie_arg.into(),
        "-d".into(), tmp_path.display().to_string(),
        url.into(),
    ];

    let mut last_err: Option<std::io::Error> = None;
    for (prog, prefix) in gallery_dl_candidates() {
        let mut args = prefix.clone();
        args.extend(base_args.clone());
        match Command::new(&prog).args(&args).output() {
            Ok(out) => return Ok((out, tmp_path.clone())),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::new(
        std::io::ErrorKind::NotFound, "gallery-dl not found"
    )))
}
