use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use kuchikiki::traits::TendrilSink;
use linkify::{LinkFinder, LinkKind};
use serde_json::Value;
use tauri::{Manager, Url};
use tauri_plugin_shell::ShellExt;
use tempfile::{NamedTempFile, TempDir};
use tokio::process::Command;
use zip::write::SimpleFileOptions;

use super::{direct, video};
use crate::database::OnDuplicate;
use crate::utils::{os::installed_browsers, url::http_url};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Link,
    Image,
    Video,
    File,
}

struct Post {
    text: String,
    assets: Vec<(Url, Kind)>,
    video_fallback: Option<Url>,
}

fn classify(url: &Url) -> Kind {
    let path = url.path().to_ascii_lowercase();
    match path.rsplit('.').next().unwrap_or("") {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp" | "svg" | "tiff" | "heic" => Kind::Image,
        "zip" | "7z" | "rar" | "pdf" | "epub" | "mp3" | "wav" | "flac" => Kind::File,
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "m3u8" | "mpd" | "ogv" => Kind::Video,
        _ => match url.host_str().unwrap_or("") {
            "youtu.be" | "player.vimeo.com" | "stream.mux.com" => Kind::Video,
            "youtube.com" | "www.youtube.com" | "www.youtube-nocookie.com" if path == "/watch" || path.starts_with("/embed/") || path.starts_with("/shorts/") => Kind::Video,
            "vimeo.com" | "www.vimeo.com" if path.trim_matches('/').rsplit('/').next().is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())) => Kind::Video,
            _ => Kind::Link,
        },
    }
}

// gallery-dl has already transferred the post's gallery and attachments with the
// selected browser session. Only inspect fields owned by this post for further assets.
fn post_contents(meta: &Value, base: &Url, downloaded: &str) -> Post {
    let mut transferred: HashSet<&str> = downloaded.lines().collect();
    for key in ["images", "attachments", "attachments_media"] {
        for item in meta[key].as_array().into_iter().flatten() {
            for key in ["download_url", "url"] {
                if let Some(url) = item[key].as_str() {
                    transferred.insert(url);
                }
            }
            if let Some(variants) = item["image_urls"].as_object() {
                transferred.extend(variants.values().filter_map(Value::as_str));
            }
        }
    }
    if let Some(url) = meta["image"]["large_url"].as_str() {
        transferred.insert(url);
    }

    let mut assets: Vec<(Url, Kind)> = Vec::new();
    let mut positions: HashMap<Url, usize> = HashMap::new();
    let mut add = |raw: &str, kind: Option<Kind>| {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') || transferred.contains(raw) {
            return;
        }
        let Ok(url) = base.join(raw) else { return };
        if http_url(url.as_str()).is_err() {
            return;
        }
        let kind = kind.unwrap_or_else(|| classify(&url));
        if let Some(&i) = positions.get(&url) {
            if assets[i].1 == Kind::Link {
                assets[i].1 = kind;
            }
        } else {
            positions.insert(url.clone(), assets.len());
            assets.push((url, kind));
        }
    };

    let is_video = meta["post_type"].as_str().unwrap_or("").contains("video");
    let file = &meta["post_file"];
    let mut video_fallback = None;
    if let Some(raw) = file["url"].as_str() {
        let kind = base.join(raw).ok().map(|url| classify(&url)).unwrap_or(Kind::File);
        if is_video || kind == Kind::Video {
            // Patreon's native stream endpoint can require authentication even on public posts.
            // yt-dlp's Patreon extractor resolves the current playable source from the permalink.
            add(base.as_str(), Some(Kind::Video));
            video_fallback = file["viewer_playback_data"]["url"].as_str().or(Some(raw)).and_then(|url| http_url(url).ok());
        } else {
            add(raw, Some(if kind == Kind::Link { Kind::File } else { kind }));
        }
    } else if let Some(url) = meta["embed"]["url"].as_str() {
        add(url, is_video.then_some(Kind::Video));
    } else if is_video {
        add(base.as_str(), Some(Kind::Video));
    }

    let dom = kuchikiki::parse_html().one(meta["content"].as_str().unwrap_or("")).document_node;
    for node in dom.select("script, style, #comments, .comments, [data-tag*='comment'], [data-testid*='comment']").unwrap() {
        node.as_node().detach();
    }
    for el in dom.select("a[href], img, picture source[srcset], video, video source, iframe[src]").unwrap() {
        let tag = el.name.local.as_ref();
        let attrs = el.attributes.borrow();
        let srcset = attrs.get("srcset").and_then(|s| s.rsplit(',').next()?.split_whitespace().next());
        if let Some(url) = attrs.get(if tag == "a" { "href" } else { "src" }).or_else(|| attrs.get("data-src")).or(srcset) {
            let kind = match tag {
                "img" => Some(Kind::Image),
                "source" if attrs.get("srcset").is_some() => Some(Kind::Image),
                "a" if attrs.get("download").is_some() => Some(Kind::File),
                "source" | "video" => Some(Kind::Video),
                _ => None,
            };
            add(url, kind);
        }
    }

    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    let mut body = Vec::new();
    for node in dom.descendants() {
        if let Some(text) = node.as_text() {
            let text = text.borrow();
            let line = text.trim();
            if !line.is_empty() {
                body.push(line.to_owned());
                if !node.ancestors().any(|n| n.as_element().is_some_and(|e| e.name.local.as_ref() == "a")) {
                    for link in finder.links(line) {
                        add(link.as_str(), None);
                    }
                }
            }
        }
    }
    Post {text: format!("{}\n{}\n\n{}\n", meta["title"].as_str().unwrap_or("Patreon post"), base, body.join("\n")), assets, video_fallback}
}

pub(super) struct PostTools {
    gallery: PathBuf,
    video: PathBuf,
    ffmpeg: Option<PathBuf>,
}

impl PostTools {
    pub(super) fn for_app(app: &tauri::AppHandle, system: bool) -> Result<Self, String> {
        let program = |name: &str| -> Result<PathBuf, String> {
            if system {
                return Ok(PathBuf::from(name));
            }
            let command = app.shell().sidecar(name).map_err(|e| e.to_string())?;
            Ok(std::process::Command::from(command).get_program().into())
        };
        let ffmpeg = if system { None } else { Some(app.path().resource_dir().map_err(|e| e.to_string())?) };
        Ok(Self {gallery: program("gallery-dl")?, video: program("yt-dlp")?, ffmpeg})
    }

    pub(super) async fn download(&self, id: &str, dest: &Path, policy: &OnDuplicate, notify: impl Fn(String)) -> Result<PathBuf, String> {
        let base = Url::parse(&format!("https://www.patreon.com/posts/{id}")).map_err(|e| e.to_string())?;
        let stem = format!("patreon-post-{id}");
        let existing = dest.join(format!("{stem}.zip"));
        if matches!(policy, OnDuplicate::DoNothing) && existing.is_file() {
            return Ok(existing);
        }

        let profiles = std::iter::once(("public access".into(), String::new()))
            .chain(std::iter::once_with(installed_browsers).flatten());
        let (stage, post, cookie) = self.fetch_post(id, &base, dest, profiles, &notify).await?;
        self.save_body_assets(stage.path(), post, &base, &cookie, &notify).await?;

        notify("Saving post archive".into());
        let directory = dest.to_owned();
        let archive = tokio::task::spawn_blocking(move || pack(stage, &directory)).await
            .map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        direct::save_file(archive, dest, &stem, "zip", policy).map_err(|e| e.to_string())
    }

    async fn fetch_post(&self, id: &str, base: &Url, dest: &Path, profiles: impl Iterator<Item = (String, String)>, notify: &impl Fn(String)) -> Result<(TempDir, Post, String), String> {
        let mut last_error = format!("No available session could open post {id}");
        for (label, cookie) in profiles {
            notify(format!("Opening post {id} using {label}"));
            let stage = TempDir::new_in(dest).map_err(|e| e.to_string())?;
            let transferred = stage.path().join("transferred.txt");
            let mut cmd = command(&self.gallery);
            cmd.args(["--config-ignore", "--no-input", "--write-info-json", "-D"]).arg(stage.path())
                .args(["-f", "gallery-{num:04}.{extension}", "-o", "extractor.patreon.files=images,image_large,attachments,content",
                       "-o", "extractor.patreon.format-images=original", "-o", "extractor.metadata-url=source_url",
                       "--Print-to-file", "after:{source_url}"]).arg(&transferred);
            if !cookie.is_empty() {
                cmd.args(["--cookies-from-browser", &cookie]);
            }
            let status = cmd.args(["--", base.as_str()]).status().await.map_err(|e| format!("Could not start gallery-dl: {e}"))?;
            if !status.success() {
                last_error = format!("gallery-dl could not read or download post {id} using {label} ({status})");
                continue;
            }
            let info = stage.path().join("info.json");
            let bytes = match tokio::fs::read(&info).await {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    last_error = format!("No full post details were available for {id} using {label}");
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            let meta: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid Patreon post metadata")?;
            if meta["id"].as_u64().map(|n| n.to_string()).or_else(|| meta["id"].as_str().map(str::to_owned)).as_deref() != Some(id) {
                return Err("Patreon returned a different post".into());
            }
            if meta["current_user_can_view"].as_bool() != Some(true) {
                last_error = format!("Post {id} requires a browser signed in with access");
                continue;
            }
            tokio::fs::remove_file(info).await.map_err(|e| e.to_string())?;
            let downloaded = match tokio::fs::read_to_string(&transferred).await {
                Ok(urls) => {
                    tokio::fs::remove_file(transferred).await.map_err(|e| e.to_string())?;
                    urls
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.to_string()),
            };
            return Ok((stage, post_contents(&meta, base, &downloaded), cookie));
        }
        Err(last_error)
    }

    async fn save_body_assets(&self, directory: &Path, post: Post, base: &Url, cookie: &str, notify: &impl Fn(String)) -> Result<(), String> {
        tokio::fs::write(directory.join("post.txt"), post.text).await.map_err(|e| e.to_string())?;
        for (index, (url, kind)) in post.assets.into_iter().enumerate() {
            let number = index + 1;
            match kind {
                Kind::Link => {
                    let shortcut = format!("[InternetShortcut]\r\nURL={}\r\n", url.as_str());
                    tokio::fs::write(directory.join(format!("link-{number:04}.url")), shortcut).await.map_err(|e| e.to_string())?;
                }
                Kind::Image | Kind::File => {
                    notify(format!("Downloading file {number} from the post"));
                    direct::download(url, directory, if kind == Kind::Image { "image" } else { "file" }, number as i64, &OnDuplicate::CreateNew).await?;
                }
                Kind::Video => {
                    notify(format!("Downloading video {number} from the post"));
                    let fallback = post.video_fallback.as_ref().filter(|_| &url == base);
                    self.save_video(directory, number, &url, fallback, base, cookie).await?;
                }
            }
        }

        Ok(())
    }

    async fn save_video(&self, directory: &Path, number: usize, url: &Url, fallback: Option<&Url>, base: &Url, cookie: &str) -> Result<(), String> {
        let expected_stem = format!("video-{number:04}");
        let mut failure = "no video file was produced".to_owned();
        for source in std::iter::once(url).chain(fallback.filter(|other| *other != url)) {
            let work = TempDir::new_in(directory).map_err(|e| e.to_string())?;
            let mut cmd = command(&self.video);
            cmd.args(video::base_ytdlp_args(Some(base.as_str()), cookie, false, false))
                .args(["--no-playlist", "--no-simulate", "-P"]).arg(work.path())
                .arg("-o").arg(format!("video-{number:04}.%(ext)s"));
            if let Some(dir) = &self.ffmpeg {
                cmd.arg("--ffmpeg-location").arg(dir);
            }
            let status = cmd.args(["--", source.as_str()]).status().await.map_err(|e| format!("Could not start yt-dlp: {e}"))?;
            if !status.success() {
                failure = status.to_string();
                continue;
            }
            for entry in fs::read_dir(work.path()).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let Some((stem, ext)) = name.to_str().and_then(|s| s.rsplit_once('.')) else { continue };
                if stem == expected_stem && matches!(ext, "mp4" | "webm" | "mkv" | "mov" | "m4v" | "ogv" | "flv" | "avi" | "ts" | "mpg" | "mpeg" | "3gp" | "wmv")
                    && entry.metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
                    fs::rename(entry.path(), directory.join(name)).map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
        }
        Err(format!("Video {number} from the post failed ({failure})"))
    }

}

fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.kill_on_drop(true).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    cmd
}

fn pack(stage: TempDir, dest: &Path) -> io::Result<tempfile::TempPath> {
    let mut temp = NamedTempFile::new_in(dest)?;
    let mut zip = zip::ZipWriter::new(temp.as_file_mut());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
    for entry in fs::read_dir(stage.path())? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        zip.start_file(entry.file_name().to_string_lossy(), options)?;
        let mut file = File::open(entry.path())?;
        io::copy(&mut file, &mut zip)?;
    }
    zip.finish()?.flush()?;
    Ok(temp.into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Read;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn full_body_selects_images_gifs_videos_and_links_once() {
        let base = Url::parse("https://www.patreon.com/posts/101").unwrap();
        let meta = json!({
            "title": "Full post",
            "content": r#"<p>Visit https://example.org/reference.</p><img src="/photo.gif?id=1&amp;sig=test"><video><source src="https://cdn.example/movie.mp4"></video><a href="https://example.org/reference">Reference</a><img src="https://cdn.example/gallery.jpg"><div data-tag="comment"><img src="/comment.gif">https://example.org/comment</div>"#,
            "images": [{"download_url": "https://cdn.example/gallery.jpg"}],
            "post_type": "video",
            "embed": {"url": "https://player.vimeo.com/video/123"},
            "comments": [{"content": "https://example.org/private-comment"}],
            "teaser_text": "https://example.org/preview"
        });
        let post = post_contents(&meta, &base, "");
        assert_eq!(post.assets.len(), 4);
        assert!(post.assets.contains(&(base.join("/photo.gif?id=1&sig=test").unwrap(), Kind::Image)));
        assert!(post.assets.contains(&(Url::parse("https://cdn.example/movie.mp4").unwrap(), Kind::Video)));
        assert!(post.assets.contains(&(Url::parse("https://player.vimeo.com/video/123").unwrap(), Kind::Video)));
        assert!(post.assets.contains(&(Url::parse("https://example.org/reference").unwrap(), Kind::Link)));
        assert!(!post.text.contains("comment"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn each_post_gets_its_own_archive_and_a_locked_post_returns_an_error() {
        use std::os::unix::fs::PermissionsExt;

        let dest = tempfile::tempdir().unwrap();
        let gallery = dest.path().join("fixture-gallery");
        fs::write(&gallery, r#"#!/bin/sh
set -eu
while [ "$1" != "-D" ]; do shift; done
dir=$2
while [ "$1" != "--" ]; do shift; done
id=${2##*/}
case "$id" in
    101|102) allowed=true ;;
    103) allowed=false ;;
    *) exit 1 ;;
esac
printf '{"id":%s,"title":"Post %s","current_user_can_view":%s,"content":"Read https://example.org/post-%s","comments":[{"content":"https://example.org/comment"}]}' "$id" "$id" "$allowed" "$id" > "$dir/info.json"
printf 'GIF89a-post-%s' "$id" > "$dir/gallery-0001.gif"
"#).unwrap();
        fs::set_permissions(&gallery, fs::Permissions::from_mode(0o700)).unwrap();
        let tools = PostTools {gallery, video: PathBuf::new(), ffmpeg: None};
        for id in ["101", "102"] {
            let file = tools.download(id, dest.path(), &OnDuplicate::CreateNew, |_| {}).await.unwrap();
            assert_eq!(file.file_name().unwrap().to_str().unwrap(), format!("patreon-post-{id}.zip"));
            let mut archive = zip::ZipArchive::new(File::open(file).unwrap()).unwrap();
            let mut gif = String::new();
            archive.by_name("gallery-0001.gif").unwrap().read_to_string(&mut gif).unwrap();
            assert_eq!(gif, format!("GIF89a-post-{id}"));
            let mut shortcut = String::new();
            archive.by_name("link-0001.url").unwrap().read_to_string(&mut shortcut).unwrap();
            assert!(shortcut.contains(&format!("https://example.org/post-{id}")));
            assert_eq!(archive.len(), 3);
        }

        let base = Url::parse("https://www.patreon.com/posts/103").unwrap();
        let result = tools.fetch_post("103", &base, dest.path(), std::iter::once(("public".into(), String::new())), &|_| {}).await;
        assert!(result.err().unwrap().contains("signed in with access"));
        assert!(!dest.path().join("patreon-post-103.zip").exists());
    }

    #[tokio::test]
    async fn archive_contains_transferred_bytes_and_post_links() {
        let directory = tempfile::tempdir().unwrap();
        let stage = TempDir::new_in(directory.path()).unwrap();
        fs::write(stage.path().join("gallery-0001.jpg"), b"gallery attachment").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gif = format!("http://{}/inline.gif", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.read(&mut [0; 2048]).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: 6\r\n\r\nGIF89a").await.unwrap();
        });
        let tools = PostTools {gallery: PathBuf::new(), video: PathBuf::new(), ffmpeg: None};
        let base = Url::parse("https://www.patreon.com/posts/101").unwrap();
        let post = post_contents(&json!({"title": "My post", "content": format!("<p>Full body</p><img src='{gif}'><a href='https://example.org/notes'>Notes</a>")}), &base, "");
        tools.save_body_assets(stage.path(), post, &base, "", &|_| {}).await.unwrap();
        let file = pack(stage, directory.path()).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(file).unwrap()).unwrap();
        assert_eq!(zip.len(), 4);
        for (name, expected) in [("gallery-0001.jpg", "gallery attachment"), ("patreon-1-inline.gif", "GIF89a"), ("link-0002.url", "URL=https://example.org/notes"), ("post.txt", "Full body")] {
            let mut body = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut body).unwrap();
            assert!(body.contains(expected));
        }
    }
}
