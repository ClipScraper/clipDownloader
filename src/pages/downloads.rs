use yew::prelude::*;
use yew_icons::{Icon, IconId};
use crate::types::{ClipRow, MediaKind, platform_str, content_type_str, Platform, ContentType};
use crate::app::DeleteItem;

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub rows: Vec<ClipRow>,
    pub on_delete: Callback<DeleteItem>,
}

/* ───────────────────────── label helpers ───────────────────────── */
fn url_after_domain(url: &str) -> String {
    let no_scheme = url.split("//").nth(1).unwrap_or(url);
    match no_scheme.find('/') {
        Some(i) => no_scheme[i + 1..].to_string(),
        None => String::new(),
    }
}

fn tik_tok_handle_from_url(url: &str) -> Option<String> {
    let tail = url_after_domain(url);
    if let Some(idx) = tail.find('@') {
        let rest = &tail[idx + 1..];
        let handle = rest.split('/').next().unwrap_or("");
        if !handle.is_empty() { return Some(handle.to_string()); }
    }
    None
}

fn instagram_handle_from_url(url: &str) -> Option<String> {
    let tail = url_after_domain(url);
    let mut it = tail.split('/');
    let first = it.next().unwrap_or("");
    if !first.is_empty() && first != "p" && first != "reel" { Some(first.to_string()) } else { None }
}

fn last_two_path_segments(url: &str) -> String {
    let tail = url_after_domain(url);
    let parts: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => tail,
        1 => parts[0].to_string(),
        _ => format!("{}/{}", parts[parts.len()-2], parts[parts.len()-1]),
    }
}

fn display_label_for_row(row: &ClipRow) -> String {
    let link = row.link.trim();
    let platform = platform_str(&row.platform);

    let mut user = row.handle.trim().to_string();
    if user.is_empty() || user.eq_ignore_ascii_case("unknown") {
        if platform == "tiktok" {
            if let Some(h) = tik_tok_handle_from_url(link) { user = h; }
        } else if platform == "instagram" {
            if let Some(h) = instagram_handle_from_url(link) { user = h; }
        }
    }

    let content = if platform == "instagram" {
        let tail = url_after_domain(link);
        let mut parts = tail.split('/').filter(|s| !s.is_empty());
        let _maybe_user = parts.next().unwrap_or_default();
        let b = parts.next().unwrap_or_default(); // "p" or "reel"
        let c = parts.next().unwrap_or_default(); // id
        if (b == "p" || b == "reel") && !c.is_empty() { format!("{}/{}", b, c) } else { last_two_path_segments(link) }
    } else if platform == "tiktok" {
        // ✅ keep the owner String alive while we hold &str slices
        let tail = url_after_domain(link);
        let pieces: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(pos) = pieces.iter().position(|p| *p == "photo" || *p == "video") {
            if pos + 1 < pieces.len() { format!("{}/{}", pieces[pos], pieces[pos + 1]) } else { last_two_path_segments(link) }
        } else {
            last_two_path_segments(link)
        }
    } else {
        last_two_path_segments(link)
    };

    if !user.is_empty() && !user.eq_ignore_ascii_case("unknown") {
        format!("{} - {}", user, content)
    } else {
        content
    }
}

/* ───────────────────────── component ───────────────────────── */

fn platform_icon_src(p: &str) -> &'static str {
    match p {
        "instagram"     => "public/instagram.webp",
        "tiktok"        => "public/tiktok.webp",
        "youtube"       => "public/youtube.webp",
        _               => "",
    }
}

#[function_component(DownloadsPage)]
pub fn downloads_page(props: &Props) -> Html {
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());

    use std::collections::{BTreeMap, HashSet};

    // platform -> (handle, type, Platform, ContentType) -> rows
    let mut map: BTreeMap<String, BTreeMap<(String, String, Platform, ContentType), Vec<ClipRow>>> = BTreeMap::new();

    // De-dupe by (platform, handle, content_type, link)
    let mut seen = HashSet::<String>::new();

    for mut r in props.rows.clone() {
        if r.handle.trim().is_empty() { r.handle = "Unknown".into(); }
        let plat = platform_str(&r.platform).to_string();
        let typ = content_type_str(&r.content_type).to_string();

        let dedup_key = format!("{}|{}|{}|{}", plat, r.handle.to_lowercase().trim(), typ, r.link.trim());
        if !seen.insert(dedup_key) {
            continue; // skip duplicates
        }

        map.entry(plat)
            .or_default()
            .entry((r.handle.clone(), typ, r.platform, r.content_type))
            .or_default()
            .push(r);
    }

    html! {
        <main class="container downloads" style="padding-top: 10vh;">
            <div class="summary">
                {
                    for map.into_iter().map(|(plat_label, mut col_map)| {
                        for rows in col_map.values_mut() {
                            rows.sort_by(|a, b| display_label_for_row(a).cmp(&display_label_for_row(b)));
                        }

                        let collections_count = col_map.len();
                        let bookmarks_count: usize = col_map.values().map(|v| v.len()).sum();

                        let key = plat_label.clone();
                        let is_open = expanded_platforms.contains(&key);

                        let on_platform_click = {
                            let expanded_platforms = expanded_platforms.clone();
                            let k = key.clone();
                            Callback::from(move |_| {
                                let mut set = (*expanded_platforms).clone();
                                if !set.insert(k.clone()) { set.remove(&k); }
                                expanded_platforms.set(set);
                            })
                        };

                        let on_delete_platform = {
                            let on_delete = props.on_delete.clone();
                            let platform = match plat_label.as_str() {
                                "instagram"     => Platform::Instagram,
                                "tiktok"        => Platform::Tiktok,
                                "youtube"       => Platform::Youtube,
                                _               => Platform::Tiktok,
                            };
                            Callback::from(move |e: MouseEvent| {
                                e.stop_propagation();
                                on_delete.emit(DeleteItem::Platform(platform));
                            })
                        };

                        let platform_rows = if is_open {
                            html! {
                                <div>
                                    {
                                        for col_map.into_iter().map(|((handle, typ_str, plat, ctype), rows)| {
                                            let col_key = format!("{}::{}::{}", plat_label, handle, typ_str);
                                            let col_open = expanded_collections.contains(&col_key);

                                            let on_col_click = {
                                                let expanded_collections = expanded_collections.clone();
                                                let k = col_key.clone();
                                                Callback::from(move |_| {
                                                    let mut set = (*expanded_collections).clone();
                                                    if !set.insert(k.clone()) { set.remove(&k); }
                                                    expanded_collections.set(set);
                                                })
                                            };

                                            let on_delete_collection = {
                                                let on_delete = props.on_delete.clone();
                                                let plat = plat.clone();
                                                let handle = handle.clone();
                                                let ctype = ctype.clone();
                                                Callback::from(move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    on_delete.emit(DeleteItem::Collection(plat.clone(), handle.clone(), ctype.clone()));
                                                })
                                            };

                                            html!{
                                                <div class="collection-block">
                                                    <div class="collection-item" onclick={on_col_click}>
                                                        <div class="item-left">
                                                            <span class="item-title">{ format!("{} | {}", handle, typ_str) }</span>
                                                        </div>
                                                        <div class="item-right">
                                                            <span>{ format!("{} bookmarks", rows.len()) }</span>
                                                            <button class="icon-btn" title="Delete" onclick={on_delete_collection}>
                                                                <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                            </button>
                                                            <button class="icon-btn" title="Download">
                                                                <img class="brand-icon" src="assets/download.svg" />
                                                            </button>
                                                        </div>
                                                    </div>

                                                    {
                                                        if col_open {
                                                            html!{
                                                                <div class="rows-card">
                                                                    <ul class="rows">
                                                                        {
                                                                            for rows.into_iter().map(|row| {
                                                                                let on_delete_row = {
                                                                                    let on_delete = props.on_delete.clone();
                                                                                    let link = row.link.clone();
                                                                                    Callback::from(move |e: MouseEvent| {
                                                                                        e.stop_propagation();
                                                                                        on_delete.emit(DeleteItem::Row(link.clone()));
                                                                                    })
                                                                                };
                                                                                html!{
                                                                                    <li class="row-line">
                                                                                        { match row.media {
                                                                                            MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"16"} height={"16"} /> },
                                                                                            MediaKind::Video    => html!{ <Icon icon_id={IconId::LucideVideo} width={"16"} height={"16"} /> },
                                                                                        }}
                                                                                        <a class="link-text" href={row.link.clone()} target="_blank">{ display_label_for_row(&row) }</a>
                                                                                        <div class="row-actions">
                                                                                            <button class="icon-btn" title="Delete" onclick={on_delete_row}>
                                                                                                <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                                            </button>
                                                                                            <button class="icon-btn" title="Download">
                                                                                                <img class="brand-icon" src="assets/download.svg" />
                                                                                            </button>
                                                                                        </div>
                                                                                    </li>
                                                                                }
                                                                            })
                                                                        }
                                                                    </ul>
                                                                </div>
                                                            }
                                                        } else { html!{} }
                                                    }
                                                </div>
                                            }
                                        })
                                    }
                                </div>
                            }
                        } else { html!{} };

                        html! {
                            <div class="platform-block">
                                <div class="platform-item" onclick={on_platform_click}>
                                    <div class="item-left">
                                        <img class="brand-icon" src={platform_icon_src(&plat_label)} />
                                        <span class="item-title">{ plat_label.clone() }</span>
                                    </div>
                                    <div class="item-right">
                                        <span>{ format!("{} collections | {} bookmarks", collections_count, bookmarks_count) }</span>
                                        <button class="icon-btn" title="Delete" onclick={on_delete_platform}>
                                            <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                        </button>
                                        <button class="icon-btn" title="Download">
                                            <img class="brand-icon" src="assets/download.svg" />
                                        </button>
                                    </div>
                                </div>
                                { platform_rows }
                            </div>
                        }
                    })
                }
            </div>
        </main>
    }
}
