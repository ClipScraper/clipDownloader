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
    fetch(url, directory, media, id, policy).await?.ok_or_else(|| "The media URL returned a webpage instead of a file".into())
}

// An extensionless download endpoint may serve an attachment or a browser landing page.
pub(super) async fn fetch(url: Url, directory: &Path, media: &str, id: i64, policy: &OnDuplicate) -> Result<Option<PathBuf>, String> {
    let client = HTTP.as_ref().map_err(|e| e.to_string())?;
    let mut response = client.get(url.clone()).send().await.map_err(|e| e.without_url().to_string())?;
    let mime = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok())
        .unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let disposition = response.headers().get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).unwrap_or("");
    let attachment = disposition.split(';').next().is_some_and(|v| v.trim().eq_ignore_ascii_case("attachment"));
    let web_page = matches!(mime.as_str(), "text/html" | "application/xhtml+xml");

    response.error_for_status_ref().map_err(|e| e.without_url().to_string())?;
    if web_page && !attachment {
        return Ok(None);
    }
    if media == "image" && !(mime.starts_with("image/") || mime == "application/octet-stream") {
        return Err(format!("Expected an image, received {mime}"));
    }

    let filename = disposition.split(';').filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| key.eq_ignore_ascii_case("filename")).map(|(_, name)| name.trim_matches('"'))
        .or_else(|| response.url().path_segments()?.next_back()).unwrap_or("").to_owned();
    let original = Path::new(&filename);
    let extension = match mime.as_str() {
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
        _ => original.extension().or_else(|| Path::new(url.path()).extension()).and_then(|e| e.to_str())
            .filter(|e| e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or("bin"),
    };
    let name = original.file_stem().and_then(|n| n.to_str()).unwrap_or("")
        .chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).take(60).collect::<String>();
    let stem = format!("patreon-{id}{}", if name.is_empty() { String::new() } else { format!("-{name}") });

    let (file, temp) = NamedTempFile::new_in(directory).map_err(|e| e.to_string())?.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    while let Some(chunk) = response.chunk().await.map_err(|e| e.without_url().to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    save_file(temp, directory, &stem, extension, policy).map(Some).map_err(|e| e.to_string())
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
    async fn downloads_signed_gif_without_changing_the_request_target() {
        let directory = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = "/4/patreon-media/p/post/101/image/1.gif?token-hash=sample%3D&token-time=123";
        let url = Url::parse(&format!("http://{}{target}", listener.local_addr().unwrap())).unwrap();
        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        let serve = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(&mut socket);
            let mut line = String::new();
            tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await.unwrap();
            assert_eq!(line.trim_end(), format!("GET {target} HTTP/1.1"));
            let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: {}\r\n\r\n", gif.len());
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(gif).await.unwrap();
        });
        let file = download(url, directory.path(), "image", 7, &OnDuplicate::CreateNew).await.unwrap();
        serve.await.unwrap();
        assert_eq!(file.extension().unwrap(), "gif");
        assert_eq!(std::fs::read(file).unwrap(), gif);
    }

    #[tokio::test]
    async fn an_extensionless_download_endpoint_uses_the_response_type() {
        use crate::utils::resource::{classify_url, Resource};
        let directory = tempfile::tempdir().unwrap();
        let html = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 6\r\n\r\n<html>").await;
        assert_eq!(classify_url(&html, "link"), Resource::FileEndpoint);
        assert!(fetch(html, directory.path(), "file", 1, &OnDuplicate::CreateNew).await.unwrap().is_none());
        let binary = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: 4\r\n\r\n%PDF").await;
        let saved = fetch(binary, directory.path(), "file", 2, &OnDuplicate::CreateNew).await.unwrap().unwrap();
        assert_eq!(std::fs::read(saved).unwrap(), b"%PDF");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn transfers_files_and_rejects_an_html_response() {
        let directory = tempfile::tempdir().unwrap();
        let image = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 5\r\n\r\nimage").await;
        let path = download(image, directory.path(), "image", 1, &OnDuplicate::CreateNew).await.unwrap();
        assert_eq!(path.extension().unwrap(), "png");
        assert_eq!(std::fs::read(path).unwrap(), b"image");

        let file = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"notes.zip\"\r\nContent-Length: 3\r\n\r\nzip").await;
        let path = download(file, directory.path(), "file", 2, &OnDuplicate::DoNothing).await.unwrap();
        assert_eq!(path.extension().unwrap(), "zip");
        assert_eq!(std::fs::read(path).unwrap(), b"zip");

        let page = fixture_response(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 6\r\n\r\n<html>").await;
        assert!(download(page, directory.path(), "image", 3, &OnDuplicate::Overwrite).await.is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
