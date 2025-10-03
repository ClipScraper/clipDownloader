use yew::prelude::*;
use yew_icons::{Icon, IconId};
use crate::types::{ClipRow, MediaKind, platform_str, content_type_str, Platform, ContentType};
use crate::app::DeleteItem;

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub rows: Vec<ClipRow>,
    pub on_delete: Callback<DeleteItem>,
}

// [FRONTEND] [pages/downloads.rs] [display_label_for_row]
// Generate a human-readable display label for a downloaded item
// Used to show a meaningful name in the downloads list instead of just the URL
// For Instagram: shows username and post type/ID
// For other platforms: shows the last two segments of the URL path
fn display_label_for_row(row: &ClipRow) -> String {
    println!("[FRONTEND] [pages/downloads.rs] [display_label_for_row]");
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

// [FRONTEND] [pages/downloads.rs] [DownloadsPage component]
// Main downloads page component that displays imported items in an organized hierarchical view
// Shows items grouped by platform (Instagram, TikTok, YouTube), then by user/channel, then by content type
// Users can expand/collapse sections and perform actions like delete or download
#[function_component(DownloadsPage)]
pub fn downloads_page(props: &Props) -> Html {
    println!("[FRONTEND] [pages/downloads.rs] [DownloadsPage component]");
    // State for tracking which platform and collection sections are expanded
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());

    // Organize imported items into a hierarchical structure for display
    // Group by: Platform -> (User Handle, Content Type) -> List of Items
    // This creates the nested expandable structure shown in the UI
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, BTreeMap<(String, String, Platform, ContentType), Vec<ClipRow>>> = BTreeMap::new();
    for r in props.rows.iter().cloned() {
        let plat = platform_str(&r.platform).to_string();
        let typ = content_type_str(&r.content_type).to_string();
        let handle = r.handle.clone();
        map.entry(plat).or_default().entry((handle, typ, r.platform, r.content_type)).or_default().push(r);
    }

    // [FRONTEND] [pages/downloads.rs] [toggle_platform callback]
    // Callback to toggle platform sections (Instagram, TikTok, YouTube) open/closed
    // Uses a set to track which platforms are currently expanded
    let toggle_platform = {
        println!("[FRONTEND] [pages/downloads.rs] [toggle_platform callback]");
        let expanded_platforms = expanded_platforms.clone();
        Callback::from(move |plat_key: String| {
            let mut set = (*expanded_platforms).clone();
            if !set.insert(plat_key.clone()) { set.remove(&plat_key); }
            expanded_platforms.set(set);
        })
    };

    // [FRONTEND] [pages/downloads.rs] [toggle_collection callback]
    // Callback to toggle collection sections (individual user/content type groups) open/closed
    // Uses a set to track which collections are currently expanded
    let toggle_collection = {
        println!("[FRONTEND] [pages/downloads.rs] [toggle_collection callback]");
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
                // Render each platform section (Instagram, TikTok, YouTube)
                // Each platform shows collection count and total item count
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
                    let on_delete_platform = {
                        let on_delete = props.on_delete.clone();
                        let platform = match plat_label.as_str() {
                            "instagram" => Platform::Instagram,
                            "tiktok" => Platform::Tiktok,
                            "youtube" => Platform::Youtube,
                            _ => Platform::Tiktok, // Should not happen
                        };
                        Callback::from(move |e: MouseEvent| {
                            e.stop_propagation();
                            on_delete.emit(DeleteItem::Platform(platform.clone()));
                        })
                    };
                    let collections_html = if is_open { html!{
                        <div>
                            { for col_map.into_iter().map(|((handle, typ_str, plat, ctype), rows)| {
                                let col_key = format!("{}::{}::{}", plat_label, handle, typ_str);
                                let col_open = expanded_collections.contains(&col_key);
                                let on_col_click = {
                                    let toggle_collection = toggle_collection.clone();
                                    let k = col_key.clone();
                                    Callback::from(move |_| toggle_collection.emit(k.clone()))
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
                                    <div>
                                        <div class="collection-item" onclick={on_col_click}>
                                            <div class="item-left">
                                                <span>{ format!("{} | {}", handle, typ_str) }</span>
                                            </div>
                                            <div class="item-right">
                                                <span>{ format!("{} bookmarks", rows.len()) }</span>
                                                <button class="icon-btn" title="Delete" onclick={on_delete_collection}><Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} /></button>
                                                <button class="icon-btn" title="Download"><img class="brand-icon" src="assets/download.svg" /></button>
                                            </div>
                                        </div>
                                        { if col_open { html!{
                                            <ul class="rows">
                                                // Render individual items within each collection
                                                // Shows media type icon, display label, and action buttons
                                                { for rows.into_iter().map(|row| {
                                                    let on_delete_row = {
                                                        let on_delete = props.on_delete.clone();
                                                        let link = row.link.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            on_delete.emit(DeleteItem::Row(link.clone()));
                                                        })
                                                    };
                                                    html!{
                                                        <li>
                                                            { match row.media {
                                                                MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"16"} height={"16"} /> },
                                                                MediaKind::Video => html!{ <Icon icon_id={IconId::LucideVideo} width={"16"} height={"16"} /> },
                                                            }}
                                                            <a class="link-text" href={row.link.clone()} target="_blank">{ display_label_for_row(&row) }</a>
                                                            <button class="icon-btn" title="Delete" onclick={on_delete_row}><Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} /></button>
                                                            <button class="icon-btn" title="Download"><img class="brand-icon" src="assets/download.svg" /></button>
                                                        </li>
                                                    }
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
                                    <button class="icon-btn" title="Delete" onclick={on_delete_platform}><Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} /></button>
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

