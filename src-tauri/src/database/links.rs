use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use rusqlite::{params, Result};
use serde::Serialize;

use super::Database;
use crate::utils::url::{http_url, link_service, patreon_post_id, text_link_url};

#[derive(Serialize)]
pub struct CollectedLink {
    platform: String,
    collection: String,
    origin: String,
    url: String,
    service: &'static str,
}

impl Database {
    pub fn record_post_links(&self, id: i64, urls: &BTreeSet<String>) -> Result<()> {
        let json = serde_json::to_string(urls).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        // Retain earlier discoveries on retry, including different MEGA fragments.
        self.conn.execute(
            "UPDATE downloads SET body_links = (SELECT json_group_array(value) FROM (
                 SELECT value FROM json_each(downloads.body_links) UNION SELECT value FROM json_each(?2)
             )) WHERE id=?1", params![id, json],
        )?;
        Ok(())
    }

    pub fn collected_links(&self) -> Result<Vec<CollectedLink>> {
        self.recover_archived_links()?;
        let mut stmt = self.conn.prepare(
            "SELECT platform, user_handle, origin, link FROM downloads
             UNION
             SELECT d.platform, d.user_handle, d.origin, j.value
               FROM downloads d, json_each(d.body_links) j",
        )?;
        let mut rows = stmt.query([])?;
        let mut links = Vec::new();
        while let Some(row) = rows.next()? {
            let url: String = row.get(3)?;
            if let Ok(parsed) = http_url(&url) {
                let mut platform: String = row.get(0)?;
                let origin: String = row.get(2)?;
                let service = link_service(&parsed);
                // Older manual submissions used YouTube as the fallback for unrecognized hosts.
                if origin == "manual" && platform == "youtube" && service != "youtube" {
                    platform = service.to_owned();
                }
                links.push(CollectedLink {
                    platform, collection: row.get(1)?, origin, service, url,
                });
            }
        }
        Ok(links)
    }

    fn recover_archived_links(&self) -> Result<()> {
        let candidates: Vec<(i64, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, link, path FROM downloads WHERE body_links IS NULL
                 AND status='done' AND lower(path) LIKE '%.zip'",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<Result<_>>()?
        };
        for (id, source, path) in candidates {
            if patreon_post_id(&source).is_none() {
                continue;
            }
            let Some(real) = crate::utils::resolve_stored_path(&path) else { continue };
            let Ok(urls) = archive_links(&real) else { continue };
            let json = serde_json::to_string(&urls).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            self.conn.execute("UPDATE downloads SET body_links=?1 WHERE id=?2 AND body_links IS NULL AND path=?3",
                params![json, id, path])?;
        }
        Ok(())
    }
}

fn archive_links(path: &Path) -> std::io::Result<BTreeSet<String>> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut found = BTreeSet::new();
    let mut finder = linkify::LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]).url_must_have_scheme(false);
    // Older post bundles contain text and Internet shortcuts alongside potentially large videos.
    // Only read the text entries created by our post writer.
    let names: Vec<String> = zip.file_names().filter(|name| {
        *name == "post.txt" || name.strip_prefix("link-").and_then(|s| s.strip_suffix(".url"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }).map(str::to_owned).collect();
    for name in names {
        let lines = BufReader::new(zip.by_name(&name)?).lines();
        if name == "post.txt" {
            // The first two lines are the title and the source post URL.
            for line in lines.skip(2) {
                found.extend(finder.links(&line?).filter_map(|link| text_link_url(link.as_str())).map(|u| u.to_string()));
            }
        } else {
            for line in lines {
                if let Some(raw) = line?.strip_prefix("URL=") {
                    if let Ok(url) = http_url(raw) {
                        found.insert(url.to_string());
                    }
                    break;
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::import::import_into;
    use rusqlite::Connection;
    use std::io::Write;

    fn database() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        super::super::init_schema(&conn).unwrap();
        Database {conn}
    }

    #[test]
    fn groups_sources_and_discoveries_without_losing_keys_or_collections() {
        let db = database();
        let mega = "https://mega.nz/file/abc#key-one";
        let csv = format!("Platform,Type,Handle,Media,link\npatreon,collection,Recipes,link,https://www.patreon.com/posts/101\npatreon,collection,Recipes,link,{mega}\ntwitter,bookmarks,Saved,link,{mega}\ndropbox,profile,Work,link,https://www.dropbox.com/s/file?key=a\ndropbox,profile,Work,link,https://www.dropbox.com/s/file?key=b\n");
        assert_eq!(import_into(&db, &csv).unwrap(), 5);
        let id = db.find_id_by_link("https://www.patreon.com/posts/101").unwrap().unwrap();
        db.record_post_links(id, &BTreeSet::from([mega.to_owned(), "https://t.me/+invite".to_owned()])).unwrap();
        db.record_post_links(id, &BTreeSet::from(["https://mega.nz/file/abc#key-two".to_owned()])).unwrap();
        let links = db.collected_links().unwrap();
        assert_eq!(links.len(), 7);
        assert!(links.iter().any(|r| r.service == "mega" && r.platform == "twitter" && r.collection == "Saved" && r.url == mega));
        assert!(links.iter().any(|r| r.service == "telegram" && r.platform == "patreon" && r.origin == "collection" && r.url.ends_with("+invite")));
        assert!(links.iter().any(|r| r.url.ends_with("#key-two")));
        assert_eq!(links.iter().filter(|r| r.service == "dropbox").count(), 2);

        db.delete_row_by_id(id).unwrap();
        assert!(!db.collected_links().unwrap().iter().any(|r| r.service == "telegram"));
    }

    #[test]
    fn recovers_and_caches_links_from_existing_post_bundles() {
        let db = database();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("patreon-post-101.zip");
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, text) in [
            ("post.txt", "Post title\nhttps://www.patreon.com/posts/101\n\nSee t.me/+invite for updates."),
            ("link-0001.url", "[InternetShortcut]\r\nURL=https://mega.nz/file/abc#the-key\r\n"),
            ("link-0002.url", "[InternetShortcut]\nURL=file:///private"),
            ("gallery-0001.gif", "https://example.org/ignore"),
        ] {
            archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            archive.write_all(text.as_bytes()).unwrap();
        }
        archive.finish().unwrap();

        import_into(&db, "Platform,Type,Handle,Media,link\npatreon,collection,Gallery,link,https://www.patreon.com/posts/101").unwrap();
        let id = db.list_backlog_ui().unwrap()[0].id;
        db.mark_id_done(id, path.to_str().unwrap()).unwrap();
        let rows = db.collected_links().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|r| r.url == "https://t.me/+invite"));
        assert!(rows.iter().any(|r| r.url == "https://mega.nz/file/abc#the-key"));
        std::fs::remove_file(path).unwrap();
        assert_eq!(db.collected_links().unwrap().len(), 3);
    }
}
