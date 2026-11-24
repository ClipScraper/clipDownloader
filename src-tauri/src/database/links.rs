use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use rusqlite::{params, Connection, Result, Transaction, TransactionBehavior};
use crate::collected_links::{CollectedLink, LinkSelection};

use super::Database;
use crate::utils::resource::{classify_url, Resource};
use crate::utils::url::{http_url, link_service, patreon_post_id, text_link_url};

impl LinkSelection {
    fn contains(&self, candidate: &CollectedLink) -> bool {
        match self {
            Self::Destination { service } => service == &candidate.service,
            Self::Collection { service, platform, collection, origin } => {
                service == &candidate.service && platform == &candidate.platform
                    && collection == &candidate.collection && origin == &candidate.origin
            }
            Self::Link { link } => link.url == candidate.url && link.platform == candidate.platform
                && link.origin == candidate.origin && link.collection == candidate.collection && link.service == candidate.service,
        }
    }
}

// Upgrade legacy rows once, before the manager resumes queued downloads.
pub(super) fn migrate_routes(conn: &Connection) -> Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? < 1 {
        let mut stmt = tx.prepare("SELECT id, link, media, status, path FROM downloads ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (id, raw, hint, status, path): (i64, String, String, String, String) =
                (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
            let Ok(url) = http_url(&raw) else { continue };
            let route = classify_url(&url, &hint);
            let shortcut = path.to_ascii_lowercase().ends_with(".url");
            let next = if status == "done" && !shortcut {
                "done"
            } else if route == Resource::Reference {
                "reference"
            } else if status == "done" || status == "reference" {
                "backlog"
            } else {
                &status
            };
            let media = if next == "done" && route == Resource::Reference { "file".to_owned() }
                else { route.media().as_str().to_owned() };
            if media != hint || next != status {
                tx.execute("UPDATE downloads SET media=?1, status=?2 WHERE id=?3", params![media, next, id])?;
            }
        }
        drop(rows);
        drop(stmt);
        // If a previous post transfer stopped after discovery, its supported body URLs
        // have no finished archive. Preserve them as actionable downloads.
        let mut pending = tx.prepare("SELECT id, body_links FROM downloads WHERE body_links IS NOT NULL AND status<>'done'")?;
        let captured = pending.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>>>()?;
        drop(pending);
        for (id, json) in captured {
            let urls = serde_json::from_str(&json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            save_discoveries(&tx, id, &urls, &urls)?;
        }
        tx.pragma_update(None, "user_version", 1)?;
    }
    tx.commit()
}

fn save_discoveries(conn: &Connection, id: i64, urls: &BTreeSet<String>, shortcuts: &BTreeSet<String>) -> Result<()> {
    let mut refs = Vec::new();
    for raw in urls {
        let Ok(url) = http_url(raw) else { continue };
        let route = classify_url(&url, "");
        if route == Resource::Reference {
            refs.push(raw);
        } else if shortcuts.contains(raw) {
            // This outbound URL would otherwise only have a shortcut in the post archive.
            // Put it through the ordinary downloader with the same collection identity.
            conn.execute(
                "INSERT INTO downloads(platform, user_handle, origin, link, name, media, status, path, date_added)
                 SELECT p.platform, p.user_handle, p.origin, ?2, ?2, ?3, 'backlog', '', CURRENT_TIMESTAMP
                 FROM downloads p WHERE p.id=?1 AND NOT EXISTS (
                     SELECT 1 FROM downloads d WHERE d.link=?2 AND
                     (d.status='done' OR (d.platform=p.platform AND d.user_handle=p.user_handle AND d.origin=p.origin))
                 )", params![id, raw, route.media().as_str()],
            )?;
        }
    }
    let json = serde_json::to_string(&refs).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    conn.execute(
        "UPDATE downloads SET body_links=(SELECT json_group_array(value) FROM (
             SELECT value FROM json_each(downloads.body_links) UNION SELECT value FROM json_each(?2)
         )), shortcuts_indexed=1 WHERE id=?1", params![id, json],
    )?;
    Ok(())
}

impl Database {
    pub fn record_post_links(&self, id: i64, urls: &BTreeSet<String>) -> Result<()> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        save_discoveries(&tx, id, urls, urls)?;
        tx.commit()
    }

    pub fn collected_links(&self) -> Result<Vec<CollectedLink>> {
        self.recover_archived_links()?;
        let mut links = BTreeSet::new();
        visit_links(&self.conn, |_, _, link| { links.insert(link); })?;
        Ok(links.into_iter().collect())
    }

    pub fn delete_collected_links(&self, selection: &LinkSelection) -> Result<()> {
        self.recover_archived_links()?;
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let mut references = Vec::new();
        let mut from_posts: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        visit_links(&tx, |id, standalone, link| {
            if selection.contains(&link) {
                if standalone {
                    references.push(id);
                } else {
                    from_posts.entry(id).or_default().push(link.url);
                }
            }
        })?;

        {
            let mut remove = tx.prepare("DELETE FROM downloads WHERE id=?1 AND status='reference'")?;
            for id in references {
                remove.execute([id])?;
            }
            let mut edit = tx.prepare(
                "UPDATE downloads SET body_links=(SELECT json_group_array(value) FROM json_each(body_links)
                 WHERE value NOT IN (SELECT value FROM json_each(?2))) WHERE id=?1",
            )?;
            for (id, urls) in from_posts {
                let removed = serde_json::to_string(&urls).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                edit.execute(params![id, removed])?;
            }
        }
        tx.commit()
    }

    pub(crate) fn recover_archived_links(&self) -> Result<bool> {
        let mut changed = false;
        let candidates: Vec<(i64, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, link, path FROM downloads WHERE shortcuts_indexed=0
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
            let Ok((urls, shortcuts)) = archive_links(&real) else { continue };
            let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
            let needed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM downloads WHERE id=?1 AND path=?2 AND shortcuts_indexed=0)", params![id, path], |r| r.get(0))?;
            if needed {
                save_discoveries(&tx, id, &urls, &shortcuts)?;
                changed = true;
            }
            tx.commit()?;
        }
        Ok(changed)
    }
}

// Listing and trash use the same visibility rules. Keep the owner ID until after
// filtering so deleting a deduplicated link can remove every copy in that collection.
fn visit_links(conn: &Connection, mut visit: impl FnMut(i64, bool, CollectedLink)) -> Result<()> {
    let mut stmt = conn.prepare(
        "WITH candidates(id, platform, user_handle, origin, url, standalone, creator) AS (
            SELECT id, platform, user_handle, origin, link, 1, creator FROM downloads_with_creator WHERE status='reference'
            UNION ALL
            SELECT d.id, d.platform, d.user_handle, d.origin, j.value, 0, d.creator FROM downloads_with_creator d, json_each(d.body_links) j
         ) SELECT id, platform, user_handle, origin, url, standalone,
                  MAX(standalone) OVER (PARTITION BY platform, user_handle, origin, url), creator
           FROM candidates c WHERE NOT EXISTS (
               SELECT 1 FROM downloads media WHERE media.link=c.url AND media.status<>'reference'
           )",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let url: String = row.get(4)?;
        let Ok(parsed) = http_url(&url) else { continue };
        if !row.get::<_, bool>(6)? && classify_url(&parsed, "") != Resource::Reference {
            continue;
        }
        let service = link_service(&parsed);
        let mut platform: String = row.get(1)?;
        let collection: String = row.get(2)?;
        let origin: String = row.get(3)?;
        // Older manual submissions used YouTube as the fallback for unrecognized hosts.
        if origin == "manual" && platform == "youtube" && service != "youtube" {
            platform = service.to_owned();
        }
        visit(row.get(0)?, row.get(5)?, CollectedLink {
            platform, collection: if collection.is_empty() { "Unknown".into() } else { collection },
            origin, url, service: service.into(), creator: row.get(7)?,
        });
    }
    Ok(())
}

fn archive_links(path: &Path) -> std::io::Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut found = BTreeSet::new();
    let mut shortcuts = BTreeSet::new();
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
                        shortcuts.insert(url.to_string());
                        found.insert(url.to_string());
                    }
                    break;
                }
            }
        }
    }
    Ok((found, shortcuts))
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
        assert_eq!(links.len(), 6);
        assert!(links.iter().any(|r| r.service == "mega" && r.platform == "twitter" && r.collection == "Saved" && r.url == mega));
        assert!(links.iter().any(|r| r.service == "telegram" && r.platform == "patreon" && r.origin == "collection" && r.url.ends_with("+invite")));
        assert!(links.iter().any(|r| r.url.ends_with("#key-two")));
        assert_eq!(links.iter().filter(|r| r.service == "dropbox").count(), 2);

        db.delete_row_by_id(id).unwrap();
        assert!(!db.collected_links().unwrap().iter().any(|r| r.service == "telegram"));
    }

    #[test]
    fn trash_is_scoped_to_destination_source_collection_and_exact_url() {
        let db = database();
        let mega = "https://mega.nz/file/example#key-one";
        let second = "https://mega.nz/file/example#key-two";
        let twitter = "https://x.com/creator";
        let patreon = "https://www.patreon.com/creator/posts?filters%5Btag%5D=art";
        let header = "Platform,Type,Handle,Media,link";
        let csv = format!("{header}\npatreon,collection,A,link,https://www.patreon.com/posts/101\npatreon,collection,A,link,https://www.patreon.com/posts/102\npatreon,collection,A,link,{mega}\ninstagram,collection,A,link,{mega}\npatreon,collection,B,link,{second}");
        import_into(&db, &csv).unwrap();
        let first = db.find_id_by_link("https://www.patreon.com/posts/101").unwrap().unwrap();
        let other = db.find_id_by_link("https://www.patreon.com/posts/102").unwrap().unwrap();
        db.record_post_links(first, &BTreeSet::from([mega, second, twitter, patreon].map(str::to_owned))).unwrap();
        db.record_post_links(other, &BTreeSet::from([mega.to_owned()])).unwrap();

        let displayed = db.collected_links().unwrap();
        assert_eq!(displayed.len(), 6);
        let groups = crate::collected_links::by_destination(&displayed);
        assert_eq!(groups.keys().copied().collect::<Vec<_>>(), ["mega", "patreon", "twitter"]);
        assert_eq!(groups["mega"][&("A", "collection", "patreon")].len(), 2);
        assert_eq!(groups["mega"][&("A", "collection", "instagram")].len(), 1);
        let link = displayed.into_iter().find(|r| r.platform == "patreon" && r.collection == "A" && r.url == mega).unwrap();
        let one = LinkSelection::Link {link};
        db.delete_collected_links(&one).unwrap();
        db.delete_collected_links(&one).unwrap();
        let left = db.collected_links().unwrap();
        assert_eq!(left.len(), 5);
        assert!(left.iter().any(|r| r.platform == "instagram" && r.url == mega));
        assert!(!left.iter().any(|r| r.platform == "patreon" && r.url == mega));
        assert!(left.iter().any(|r| r.collection == "A" && r.url == second));

        db.delete_collected_links(&LinkSelection::Collection {
            service: "mega".into(), platform: "patreon".into(), collection: "A".into(), origin: "collection".into(),
        }).unwrap();
        let left = db.collected_links().unwrap();
        assert_eq!(left.len(), 4);
        assert!(left.iter().any(|r| r.collection == "B" && r.url == second));
        assert!(left.iter().any(|r| r.collection == "A" && r.url == twitter));
        assert!(left.iter().any(|r| r.collection == "A" && r.url == patreon));
        db.delete_collected_links(&LinkSelection::Destination {service: "mega".into()}).unwrap();
        let left = db.collected_links().unwrap();
        assert_eq!(left.len(), 2);
        assert!(left.iter().all(|r| r.service != "mega"));
        assert_eq!(db.list_backlog_ui().unwrap().len(), 2);
    }

    #[test]
    fn references_media_and_completed_files_use_distinct_views() {
        let db = database();
        let gif = "https://c10.patreonusercontent.com/4/patreon-media/p/post/101/hash/size/1.gif?token-hash=sample%3D&token-time=123";
        let reel = "https://www.instagram.com/reel/abc/";
        let tiktok = "https://www.tiktok.com/@creator/video/123";
        let external = "https://x.com/creator";
        let post = "https://www.patreon.com/posts/101";
        let csv = format!("Platform,Type,Handle,Media,link\npatreon,collection,Gallery,link,{post}\npatreon,collection,Gallery,link,{gif}\npatreon,collection,Gallery,link,{external}");
        assert_eq!(import_into(&db, &csv).unwrap(), 3);
        let id = db.find_id_by_link(post).unwrap().unwrap();
        let discovered = [gif, reel, tiktok, external].map(str::to_owned).into_iter().collect();
        for _ in 0..2 { db.record_post_links(id, &discovered).unwrap(); }
        let backlog = db.list_backlog_ui().unwrap();
        assert_eq!(backlog.len(), 4);
        assert!(backlog.iter().any(|r| r.link == gif && r.media == "pictures"));
        for url in [reel, tiktok] {
            assert!(backlog.iter().any(|r| r.link == url && r.media == "video" && r.handle == "Gallery"));
        }
        let links = db.collected_links().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, external);

        assert_eq!(db.move_collection_to_queue("patreon", "Gallery", "collection").unwrap(), 4);
        db.mark_id_done(db.find_id_by_link(gif).unwrap().unwrap(), "/downloads/1.gif").unwrap();
        assert_eq!(db.list_done_ui().unwrap()[0].link, gif);
        assert_eq!(db.list_queue_ui().unwrap().len(), 3);
        assert_eq!(db.collected_links().unwrap().len(), 1);
        // A URL once believed to be a reference may already have produced a real file in another collection.
        db.conn.execute("INSERT INTO downloads(platform,name,media,user_handle,origin,link,status,path,date_added) VALUES('other','file','file','Other','manual',?1,'done','saved.pdf','today')", [external]).unwrap();
        assert!(db.collected_links().unwrap().is_empty());
    }

    #[test]
    fn upgrades_old_rows_without_redownloading_completed_media() {
        let db = database();
        db.conn.pragma_update(None, "user_version", 0).unwrap();
        for (url, status, path) in [
            ("https://cdn.example/one.gif?sig=one", "done", "one.gif"),
            ("https://cdn.example/two.gif?sig=two", "done", "old.url"),
            ("https://www.instagram.com/reel/abc/", "queued", ""),
            ("https://t.me/+example", "queued", ""),
            ("https://mega.nz/file/abc#key", "done", "share.url"),
        ] {
            db.conn.execute("INSERT INTO downloads(platform,name,media,user_handle,origin,link,status,path,date_added) VALUES('patreon','x','link','Collection','collection',?1,?2,?3,'today')", [url,status,path]).unwrap();
        }
        for _ in 0..2 { super::migrate_routes(&db.conn).unwrap(); }
        assert_eq!(db.list_done_ui().unwrap().len(), 1);
        assert_eq!(db.list_done_ui().unwrap()[0].media, "pictures");
        assert_eq!(db.list_backlog_ui().unwrap()[0].link, "https://cdn.example/two.gif?sig=two");
        assert_eq!(db.list_queue_ui().unwrap()[0].media, "video");
        assert_eq!(db.collected_links().unwrap().len(), 2);
        let reference = db.find_id_by_link("https://t.me/+example").unwrap().unwrap();
        assert_eq!(db.set_status_by_id(reference, crate::database::DownloadStatus::Queued).unwrap(), 0);
    }

    #[test]
    fn recovers_and_caches_links_from_existing_post_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("catalog.db");
        let db = Database {conn: Connection::open(&db_file).unwrap()};
        super::super::init_schema(&db.conn).unwrap();
        let path = dir.path().join("patreon-post-101.zip");
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, text) in [
            ("post.txt", "Post title\nhttps://www.patreon.com/posts/101\n\nSee t.me/+invite for updates."),
            ("link-0001.url", "[InternetShortcut]\r\nURL=https://mega.nz/file/abc#the-key\r\n"),
            ("link-0002.url", "[InternetShortcut]\nURL=file:///private"),
            ("link-0003.url", "[InternetShortcut]\nURL=https://www.instagram.com/reel/recovered/"),
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
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.url == "https://t.me/+invite"));
        assert!(rows.iter().any(|r| r.url == "https://mega.nz/file/abc#the-key"));
        assert_eq!(db.list_backlog_ui().unwrap()[0].link, "https://www.instagram.com/reel/recovered/");
        db.delete_collected_links(&LinkSelection::Destination {service: "mega".into()}).unwrap();
        assert!(path.is_file());
        assert_eq!(db.list_done_ui().unwrap()[0].id, id);
        assert_eq!(db.collected_links().unwrap().len(), 1);
        drop(db);
        let reopened = Database {conn: Connection::open(db_file).unwrap()};
        assert_eq!(reopened.collected_links().unwrap()[0].service, "telegram");
        std::fs::remove_file(path).unwrap();
        assert_eq!(reopened.collected_links().unwrap().len(), 1);
    }
}
