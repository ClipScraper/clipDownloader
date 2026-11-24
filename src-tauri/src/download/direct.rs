use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::database::OnDuplicate;
use once_cell::sync::Lazy;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use reqwest::{Client, Url};
use tempfile::{NamedTempFile, TempPath};
use tokio::io::AsyncWriteExt;

static HTTP: Lazy<Result<Client, reqwest::Error>> = Lazy::new(|| {
    Client::builder().connect_timeout(Duration::from_secs(15)).read_timeout(Duration::from_secs(60)).build()
});

pub(super) fn save_file(mut temp: TempPath, directory: &Path, stem: &str, ext: &str, policy: &OnDuplicate) -> io::Result<PathBuf> {
    let mut target = directory.join(format!("{stem}.{ext}"));
    if matches!(policy, OnDuplicate::Overwrite) {
        temp.persist(&target)?;
        return Ok(target);
    }
    for suffix in 2.. {
        match temp.persist_noclobber(&target) {
            Ok(()) => return Ok(target),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                if matches!(policy, OnDuplicate::DoNothing) {
                    return Ok(target);
                }
                temp = e.path;
                target = directory.join(format!("{stem} ({suffix}).{ext}"));
            }
            Err(e) => return Err(e.error),
        }
    }
    unreachable!()
}

pub(super) async fn download(url: Url, directory: &Path, media: &str, id: i64, policy: &OnDuplicate) -> Result<PathBuf, String> {
    let client = HTTP.as_ref().map_err(|e| e.to_string())?;
    let mut response = client.get(url.clone()).send().await.map_err(|e| e.without_url().to_string())?;
    let mime = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok())
        .unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let disposition = response.headers().get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).unwrap_or("");
    let attachment = disposition.split(';').next().is_some_and(|v| v.trim().eq_ignore_ascii_case("attachment"));
    let web_page = matches!(mime.as_str(), "text/html" | "application/xhtml+xml");
    let bookmark = media == "link" && ((web_page && !attachment) || !response.status().is_success());

    if !bookmark {
        response.error_for_status_ref().map_err(|e| e.without_url().to_string())?;
        if media == "file" && web_page && !attachment {
            return Err("The attachment returned a webpage instead of a file".into());
        }
        if media == "image" && !(mime.starts_with("image/") || mime == "application/octet-stream") {
            return Err(format!("Expected an image, received {mime}"));
        }
    }

    let filename = disposition.split(';').filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| key.eq_ignore_ascii_case("filename")).map(|(_, name)| name.trim_matches('"'))
        .or_else(|| response.url().path_segments()?.next_back()).unwrap_or("").to_owned();
    let original = Path::new(&filename);
    let extension = if bookmark {
        "url"
    } else {
        match mime.as_str() {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "image/avif" => "avif",
            "image/svg+xml" => "svg",
            "application/pdf" => "pdf",
            "application/zip" => "zip",
            "text/plain" => "txt",
            "video/mp4" => "mp4",
            "audio/mpeg" => "mp3",
            _ => original.extension().and_then(|e| e.to_str())
                .filter(|e| e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or("bin"),
        }
    };
    let name = original.file_stem().and_then(|n| n.to_str()).unwrap_or("")
        .chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).take(60).collect::<String>();
    let stem = format!("patreon-{id}{}", if name.is_empty() { String::new() } else { format!("-{name}") });

    let (file, temp) = NamedTempFile::new_in(directory).map_err(|e| e.to_string())?.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    if bookmark {
        file.write_all(format!("[InternetShortcut]\r\nURL={}\r\n", url.as_str()).as_bytes()).await.map_err(|e| e.to_string())?;
    } else {
        while let Some(chunk) = response.chunk().await.map_err(|e| e.without_url().to_string())? {
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    save_file(temp, directory, &stem, extension, policy).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    async fn fixture_response(response: &'static [u8]) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            socket.read(&mut request).await.unwrap();
            socket.write_all(response).await.unwrap();
        });
        Url::parse(&format!("http://{address}/file?id=1&signature=a%2Bb#section")).unwrap()
    }

    #[tokio::test]
    async fn transfers_direct_media_and_saves_webpages_as_shortcuts() {
        let directory = tempfile::tempdir().unwrap();
        let image = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 5\r\n\r\nimage").await;
        let path = download(image, directory.path(), "image", 1, &OnDuplicate::CreateNew).await.unwrap();
        assert_eq!(path.extension().unwrap(), "png");
        assert_eq!(std::fs::read(path).unwrap(), b"image");

        let file = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"notes.zip\"\r\nContent-Length: 3\r\n\r\nzip").await;
        let path = download(file, directory.path(), "link", 2, &OnDuplicate::DoNothing).await.unwrap();
        assert_eq!(path.extension().unwrap(), "zip");
        assert_eq!(std::fs::read(path).unwrap(), b"zip");

        let page = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 6\r\n\r\n<html>").await;
        let path = download(page.clone(), directory.path(), "link", 3, &OnDuplicate::Overwrite).await.unwrap();
        assert_eq!(path.extension().unwrap(), "url");
        assert_eq!(std::fs::read_to_string(path).unwrap(), format!("[InternetShortcut]\r\nURL={page}\r\n"));
    }
}
