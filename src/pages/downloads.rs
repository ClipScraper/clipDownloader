use yew::prelude::*;
use yew_icons::{Icon, IconId};
use crate::types::{ClipRow, MediaKind, platform_str, content_type_str, Platform};

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub rows: Vec<ClipRow>,
}

fn display_label_for_row(row: &ClipRow) -> String {
    let url = row.link.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() >= 6 && (parts[3] == "www.instagram.com" || parts[2].contains("instagram.com")) {
        if parts.len() >= 6 && parts[4] != "p" {
            let username = parts[4];
            let kind = parts.get(5).unwrap_or(&"");
            let id = parts.get(6).unwrap_or(&"");
            let id = if id.is_empty() { parts.last().unwrap_or(&"") } else { id };
            if kind == id { return format!("{} - {}", username, kind); }
            return format!("{} - {} - {}", username, kind, id);
        }
    }
    if parts.len() >= 2 {
        let a = parts[parts.len()-2];
        let b = parts[parts.len()-1];
        if a == b { return a.to_string(); }
        return format!("{}/{}", a, b);
    }
    row.link.clone()
}

fn platform_icon_src(p: &str) -> &'static str {
    match p {
        "instagram" => "public/instagram.webp",
        "tiktok" => "public/tiktok.webp",
        "youtube" => "public/youtube.webp",
        _ => "",
    }
}

#[function_component(DownloadsPage)]
pub fn downloads_page(props: &Props) -> Html {
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());

    // Build summaries inline (dup of grouping kept minimal for brevity)
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, BTreeMap<(String, String), Vec<ClipRow>>> = BTreeMap::new();
    for r in props.rows.iter().cloned() {
        let plat = platform_str(&r.platform).to_string();
        let typ = content_type_str(&r.content_type).to_string();
        let handle = r.handle.clone();
        map.entry(plat).or_default().entry((handle, typ)).or_default().push(r);
    }

    let toggle_platform = {
        let expanded_platforms = expanded_platforms.clone();
        Callback::from(move |plat_key: String| {
            let mut set = (*expanded_platforms).clone();
            if !set.insert(plat_key.clone()) { set.remove(&plat_key); }
            expanded_platforms.set(set);
        })
    };
    let toggle_collection = {
        let expanded_collections = expanded_collections.clone();
        Callback::from(move |col_key: String| {
            let mut set = (*expanded_collections).clone();
            if !set.insert(col_key.clone()) { set.remove(&col_key); }
            expanded_collections.set(set);
        })
    };

    html! {
        <main class="container" style="padding-top: 10vh;">
            <div class="summary">
                { for map.into_iter().map(|(plat_label, col_map)| {
                    let collections_count = col_map.len();
                    let bookmarks_count: usize = col_map.values().map(|v| v.len()).sum();
                    let key = plat_label.clone();
                    let is_open = expanded_platforms.contains(&key);
                    let on_click = {
                        let toggle_platform = toggle_platform.clone();
                        let k = key.clone();
                        Callback::from(move |_| toggle_platform.emit(k.clone()))
                    };
                    let collections_html = if is_open { html!{
                        <div>
                            { for col_map.into_iter().map(|((handle, typ_str), rows)| {
                                let col_key = format!("{}::{}::{}", plat_label, handle, typ_str);
                                let col_open = expanded_collections.contains(&col_key);
                                let on_col_click = {
                                    let toggle_collection = toggle_collection.clone();
                                    let k = col_key.clone();
                                    Callback::from(move |_| toggle_collection.emit(k.clone()))
                                };
                                html!{
                                    <div>
                                        <div class="collection-item" onclick={on_col_click}>
                                            <div class="item-left">
                                                <span>{ format!("{} | {}", handle, typ_str) }</span>
                                            </div>
                                            <div class="item-right">
                                                <span>{ format!("{} bookmarks", rows.len()) }</span>
                                                <button class="icon-btn" title="Download"><img class="brand-icon" src="assets/download.svg" /></button>
                                            </div>
                                        </div>
                                        { if col_open { html!{
                                            <ul class="rows">
                                                { for rows.into_iter().map(|row| html!{
                                                    <li>
                                                        { match row.media {
                                                            MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"16"} height={"16"} /> },
                                                            MediaKind::Video => html!{ <Icon icon_id={IconId::LucideVideo} width={"16"} height={"16"} /> },
                                                        }}
                                                        <a class="link-text" href={row.link.clone()} target="_blank">{ display_label_for_row(&row) }</a>
                                                        <button class="icon-btn" title="Download"><img class="brand-icon" src="assets/download.svg" /></button>
                                                    </li>
                                                }) }
                                            </ul>
                                        }} else { html!{} }}
                                    </div>
                                }
                            }) }
                        </div>
                    }} else { html!{} };

                    html!{
                        <div>
                            <div class="platform-item" onclick={on_click}>
                                <div class="item-left">
                                    <img class="brand-icon" src={platform_icon_src(&plat_label)} />
                                    <span>{ plat_label.clone() }</span>
                                </div>
                                <div class="item-right">
                                    <span>{ format!("{} collections | {} bookmarks", collections_count, bookmarks_count) }</span>
                                    <button class="icon-btn" title="Download"><img class="brand-icon" src="assets/download.svg" /></button>
                                </div>
                            </div>
                            { collections_html }
                        </div>
                    }
                }) }
            </div>
        </main>
    }
}

