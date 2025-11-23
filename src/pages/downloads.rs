use crate::app::{DeleteItem, MoveItem};
use crate::types::{content_type_str, platform_str, ClipRow, ContentType, MediaKind, Platform};
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_icons::{Icon, IconId};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}
fn toggle_icon_for_row(row: &ClipRow) -> IconId {
    // If row carries explicit output_format, prefer showing music icon for audio
    if let Some(fmt) = &row.output_format {
        if fmt.eq_ignore_ascii_case("audio") {
            return IconId::LucideMusic;
        }
        if fmt.eq_ignore_ascii_case("video") {
            return icon_for_row(row);
        }
    }
    icon_for_row(row)
}

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub backlog: Vec<ClipRow>,
    pub queue: Vec<ClipRow>,
    pub active: Vec<ActiveDownload>,
    pub paused: bool,
    pub on_toggle_pause: Callback<()>,
    pub on_delete: Callback<DeleteItem>,
    pub on_move_to_queue: Callback<MoveItem>,
    pub on_move_to_backlog: Callback<crate::app::MoveBackItem>,
}

#[derive(Clone, PartialEq)]
pub struct ActiveDownload {
    pub row: ClipRow,
    pub progress: String, // ignored in UI per requirements
}

/* ───────────────────────── label helpers ───────────────────────── */

fn url_after_domain(url: &str) -> String {
    let no_scheme = url.split("//").nth(1).unwrap_or(url);
    match no_scheme.find('/') {
        Some(i) => no_scheme[i + 1..].to_string(),
        None => String::new(),
    }
}

fn last_two_path_segments(url: &str) -> String {
    let tail = url_after_domain(url);
    let parts: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => tail,
        1 => parts[0].to_string(),
        _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
    }
}

/// Collection display name: "{handle} | {type}"
fn collection_title(row: &ClipRow) -> String {
    let handle = if row.handle.trim().is_empty() {
        "Unknown"
    } else {
        &row.handle
    };
    let typ = content_type_str(&row.content_type);
    format!("{handle} | {typ}")
}

fn item_label_for_row(row: &ClipRow) -> String {
    let link = row.link.trim();
    let platform = platform_str(&row.platform);
    if platform == "instagram" {
        let tail = url_after_domain(link);
        let mut parts = tail.split('/').filter(|s| !s.is_empty());
        let _maybe_user = parts.next().unwrap_or_default();
        let b = parts.next().unwrap_or_default(); // "p" or "reel"
        let c = parts.next().unwrap_or_default(); // id
        if (b == "p" || b == "reel") && !c.is_empty() {
            format!("{b}/{c}")
        } else {
            last_two_path_segments(link)
        }
    } else if platform == "tiktok" {
        let tail = url_after_domain(link);
        let pieces: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(pos) = pieces.iter().position(|p| *p == "photo" || *p == "video") {
            if pos + 1 < pieces.len() {
                format!("{}/{}", pieces[pos], pieces[pos + 1])
            } else {
                last_two_path_segments(link)
            }
        } else {
            last_two_path_segments(link)
        }
    } else {
        last_two_path_segments(link)
    }
}

/* ───────────────────────── helpers ───────────────────────── */

fn is_instagram_photo(link: &str) -> bool {
    let tail = url_after_domain(link);
    let mut parts = tail.split('/').filter(|s| !s.is_empty());
    let _maybe_user = parts.next().unwrap_or_default();
    let b = parts.next().unwrap_or_default(); // "p" or "reel"
    b == "p"
}

fn is_tiktok_photo(link: &str) -> bool {
    let tail = url_after_domain(link);
    let pieces: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    pieces.iter().any(|p| *p == "photo")
}

fn icon_for_row(row: &ClipRow) -> IconId {
    let plat = platform_str(&row.platform);
    let link = row.link.trim();
    // Pinterest: always show as Image icon (treat all pins as images in UI)
    if plat == "pinterest" {
        return IconId::LucideImage;
    }
    if (plat == "instagram" && is_instagram_photo(link))
        || (plat == "tiktok" && is_tiktok_photo(link))
    {
        IconId::LucideImage
    } else {
        match row.media {
            MediaKind::Pictures => IconId::LucideImage,
            MediaKind::Video => IconId::LucideVideo,
        }
    }
}

fn platform_icon_src(p: &str) -> &'static str {
    match p {
        "instagram" => "public/instagram.webp",
        "pinterest" => "public/pinterest.png",
        "tiktok" => "public/tiktok.webp",
        "youtube" => "public/youtube.webp",
        _ => "",
    }
}

/* ───────────────────────── component ───────────────────────── */
#[function_component(DownloadsPage)]
pub fn downloads_page(props: &Props) -> Html {
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());
    // Local overrides so icon flips instantly on click (DB persists separately)
    let output_overrides = use_state(|| std::collections::HashMap::<String, String>::new());

    let on_toggle_pause_click_header = {
        let cb = props.on_toggle_pause.clone();
        Callback::from(move |_e: MouseEvent| cb.emit(()))
    };
    let on_toggle_pause_click_row = {
        let cb = props.on_toggle_pause.clone();
        Callback::from(move |_e: MouseEvent| cb.emit(()))
    };

    let render_section = {
        let expanded_platforms = expanded_platforms.clone();
        let expanded_collections = expanded_collections.clone();
        let on_delete_prop = props.on_delete.clone();
        let on_move_prop = props.on_move_to_queue.clone();
        let on_move_back_prop = props.on_move_to_backlog.clone();
        let output_overrides = output_overrides.clone();

        move |rows_in: Vec<ClipRow>, title: &str, enable_queue_action: bool| -> Html {
            use std::collections::{BTreeMap, HashSet};

            let section_id = title.to_lowercase(); // "backlog" or "queue"

            // platform -> (handle, type, Platform, ContentType) -> rows
            let mut map: BTreeMap<
                String,
                BTreeMap<(String, String, Platform, ContentType), Vec<ClipRow>>,
            > = BTreeMap::new();

            // De-dupe by (platform, handle, type, link) within this section
            let mut seen = HashSet::<String>::new();

            for mut r in rows_in {
                if r.handle.trim().is_empty() {
                    r.handle = "Unknown".into();
                }
                let plat = platform_str(&r.platform).to_string();
                let typ = content_type_str(&r.content_type).to_string();

                let dedup_key = format!(
                    "{}|{}|{}|{}",
                    plat,
                    r.handle.to_lowercase().trim(),
                    typ,
                    r.link.trim()
                );
                if !seen.insert(dedup_key) {
                    continue;
                }

                map.entry(plat)
                    .or_default()
                    .entry((r.handle.clone(), typ, r.platform, r.content_type))
                    .or_default()
                    .push(r);
            }

            html! {
                <>
                    <h2 style="margin: 24px 0 8px 16px;">{ title }</h2>
                    <div class="summary">
                        {
                            for map.into_iter().map(|(plat_label, mut col_map)| {
                                for rows in col_map.values_mut() {
                                    rows.sort_by(|a, b| item_label_for_row(a).cmp(&item_label_for_row(b)));
                                }

                                let collections_count = col_map.len();
                                let bookmarks_count: usize = col_map.values().map(|v| v.len()).sum();

                                /* ---- platform open state (namespaced) ---- */
                                let platform_key = format!("{}::{}", section_id, plat_label);
                                let is_open = expanded_platforms.contains(&platform_key);

                                let on_platform_click = {
                                    let expanded_platforms = expanded_platforms.clone();
                                    let k = platform_key.clone();
                                    Callback::from(move |_| {
                                        let mut set = (*expanded_platforms).clone();
                                        if !set.insert(k.clone()) { set.remove(&k); }
                                        expanded_platforms.set(set);
                                    })
                                };

                                let on_delete_platform = {
                                    let on_delete = on_delete_prop.clone();
                                    let platform = match plat_label.as_str() {
                                        "instagram" => Platform::Instagram,
                                        "tiktok" => Platform::Tiktok,
                                        "youtube" => Platform::Youtube,
                                        "pinterest" => Platform::Pinterest,
                                        _ => Platform::Tiktok,
                                    };
                                    // Backend deletion honoring delete mode
                                    let platform_str_for_backend = plat_label.clone();
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        let platform_str_for_backend = platform_str_for_backend.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "platform": platform_str_for_backend })).unwrap();
                                            let _ = invoke("delete_rows_by_platform", args).await;
                                        });
                                        on_delete.emit(DeleteItem::Platform(platform.clone()));
                                    })
                                };

                                let on_queue_platform = {
                                    let on_move = on_move_prop.clone();
                                    let platform = match plat_label.as_str() {
                                        "instagram" => Platform::Instagram,
                                        "tiktok" => Platform::Tiktok,
                                        "youtube" => Platform::Youtube,
                                        "pinterest" => Platform::Pinterest,
                                        _ => Platform::Tiktok,
                                    };
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        if enable_queue_action { on_move.emit(MoveItem::Platform(platform)); }
                                    })
                                };

                                let on_back_platform = {
                                    let on_move_back = on_move_back_prop.clone();
                                    let platform = match plat_label.as_str() {
                                        "instagram" => Platform::Instagram,
                                        "tiktok" => Platform::Tiktok,
                                        "youtube" => Platform::Youtube,
                                        "pinterest" => Platform::Pinterest,
                                        _ => Platform::Tiktok,
                                    };
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        if !enable_queue_action { on_move_back.emit(crate::app::MoveBackItem::Platform(platform)); }
                                    })
                                };

                                let platform_rows = if is_open {
                                    html! {
                                        <div>
                                            {
                                                for col_map.into_iter().map(|((handle, typ_str, plat, ctype), rows)| {
                                                    /* ---- collection open state (namespaced) ---- */
                                                    let col_key = format!("{}::{}::{}::{}", section_id, plat_label, handle, typ_str);
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
                                                        let on_delete = on_delete_prop.clone();
                                                        let plat = plat.clone();
                                                        let handle = handle.clone();
                                                        let ctype = ctype.clone();
                                                        // Backend deletion honoring delete mode
                                                        let plat_label_s = plat_label.clone();
                                                        let handle_s = handle.clone();
                                                        let typ_s = typ_str.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            let plat_label_s = plat_label_s.clone();
                                                            let handle_s = handle_s.clone();
                                                            let typ_s = typ_s.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                    "platform": plat_label_s,
                                                                    "handle": handle_s,
                                                                    "origin": typ_s,
                                                                })).unwrap();
                                                                let _ = invoke("delete_rows_by_collection", args).await;
                                                            });
                                                            on_delete.emit(DeleteItem::Collection(plat.clone(), handle.clone(), ctype.clone()));
                                                        })
                                                    };

                                                    let on_queue_collection = {
                                                        let on_move = on_move_prop.clone();
                                                        let plat_label_s = plat_label.clone();
                                                        let handle_s = handle.clone();
                                                        let typ_s = typ_str.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            if enable_queue_action {
                                                                on_move.emit(MoveItem::Collection(
                                                                    match plat_label_s.as_str() {
                                                                        "instagram"         => Platform::Instagram,
                                                                        "tiktok"            => Platform::Tiktok,
                                                                        "youtube"           => Platform::Youtube,
                                                                        "pinterest"         => Platform::Pinterest,
                                                                        _                   => Platform::Tiktok,
                                                                    },
                                                                    handle_s.clone(),
                                                                    match typ_s.as_str() {
                                                                        "liked"             => ContentType::Liked,
                                                                        "reposts"           => ContentType::Reposts,
                                                                        "profile"           => ContentType::Profile,
                                                                        "bookmarks"         => ContentType::Bookmarks,
                                                                        "playlist"          => ContentType::Playlist,
                                                                        "recommendation"    => ContentType::Recommendation,
                                                                        "manual"            => ContentType::Manual,
                                                                        "pinboard"          => ContentType::Pinboard,
                                                                        _                   => ContentType::Other,
                                                                    }
                                                                ));
                                                            }
                                                        })
                                                    };

                                                    let on_back_collection = {
                                                        let on_move_back = on_move_back_prop.clone();
                                                        let plat_label_s = plat_label.clone();
                                                        let handle_s = handle.clone();
                                                        let typ_s = typ_str.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            if !enable_queue_action {
                                                                on_move_back.emit(crate::app::MoveBackItem::Collection(
                                                                    match plat_label_s.as_str() {
                                                                        "instagram"         => Platform::Instagram,
                                                                        "tiktok"            => Platform::Tiktok,
                                                                        "youtube"           => Platform::Youtube,
                                                                        "pinterest"         => Platform::Pinterest,
                                                                        _                   => Platform::Tiktok,
                                                                    },
                                                                    handle_s.clone(),
                                                                    match typ_s.as_str() {
                                                                        "liked"             => ContentType::Liked,
                                                                        "reposts"           => ContentType::Reposts,
                                                                        "profile"           => ContentType::Profile,
                                                                        "bookmarks"         => ContentType::Bookmarks,
                                                                        "playlist"          => ContentType::Playlist,
                                                                        "recommendation"    => ContentType::Recommendation,
                                                                        "manual"            => ContentType::Manual,
                                                                        "pinboard"          => ContentType::Pinboard,
                                                                        _                   => ContentType::Other,
                                                                    }
                                                                ));
                                                            }
                                                        })
                                                    };

                                                    html!{
                                                        <div class="collection-block" key={col_key.clone()}>
                                                            <div class="collection-item" onclick={on_col_click}>
                                                                <div class="item-left">
                                                                    <span class="item-title">{ format!("{} | {}", handle, typ_str) }</span>
                                                                </div>
                                                                <div class="item-right">
                                                                    <span>{ format!("{} bookmarks", rows.len()) }</span>
                                                                    <button class="icon-btn" type_="button" title="Delete" onclick={on_delete_collection}>
                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                    </button>
                                                                    { if !enable_queue_action { html!{ <button class="icon-btn" type_="button" title="Move back to backlog" onclick={on_back_collection.clone()}><Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} /></button> } } else { html!{} } }
                                                                    {
                                                                        if enable_queue_action {
                                                                            html!{
                                                                                <button class="icon-btn" type_="button" title="Queue" onclick={on_queue_collection.clone()}>
                                                                                    <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                                                </button>
                                                                            }
                                                                        } else {
                                                                            html!{}
                                                                        }
                                                                    }
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
                                                                                            let on_delete = on_delete_prop.clone();
                                                                                            let link = row.link.clone();
                                                                                            // Backend delete honoring delete mode (delete a single link row in done/backlog/queue is always safe)
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                let link_for_backend = link.clone();
                                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "link": link_for_backend })).unwrap();
                                                                                                    let _ = invoke("delete_rows_by_link", args).await;
                                                                                                });
                                                                                                on_delete.emit(DeleteItem::Row(link.clone()));
                                                                                            })
                                                                                        };
                                                                                        let on_queue_row = {
                                                                                            let on_move = on_move_prop.clone();
                                                                                            let link = row.link.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                if enable_queue_action { on_move.emit(MoveItem::Row(link.clone())); }
                                                                                            })
                                                                                        };
                                                                                        // Determine current effective format (override -> row value)
                                                                                        let effective_fmt: Option<String> = (*output_overrides).get(&row.link).cloned().or_else(|| row.output_format.clone());
                                                                                        let row_icon = if effective_fmt.as_deref() == Some("audio") { IconId::LucideMusic } else { icon_for_row(&row) };
                                                                                        let on_click_toggle = {
                                                                                            let link_for_backend = row.link.clone();
                                                                                            let output_overrides = output_overrides.clone();
                                                                                            let current_fmt = effective_fmt.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                // Flip locally first for instant UI feedback
                                                                                                let mut map = (*output_overrides).clone();
                                                                                                let next = if current_fmt.as_deref() == Some("audio") { "video" } else { "audio" };
                                                                                                map.insert(link_for_backend.clone(), next.to_string());
                                                                                                output_overrides.set(map);
                                                                                                // Persist to DB
                                                                                                let link_for_backend = link_for_backend.clone();
                                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "link": link_for_backend })).unwrap();
                                                                                                    let _ = invoke("toggle_output_format", args).await;
                                                                                                });
                                                                                            })
                                                                                        };
                                                                                        html!{
                                                                                            <li class="row-line" key={row.link.clone()}>
                                                                                                <span onclick={on_click_toggle.clone()}><Icon icon_id={row_icon} width={"16"} height={"16"} /></span>
                                                                                                <a class="link-text" href={row.link.clone()} target="_blank">
                                                                                                    { item_label_for_row(&row) }
                                                                                                </a>
                                                                                                <div class="row-actions">
                                                                                                    <button class="icon-btn" type_="button" title="Delete" onclick={on_delete_row}>
                                                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                                                    </button>
                                                                                                    { if !enable_queue_action { html!{ <button class="icon-btn" type_="button" title="Move back to backlog" onclick={
                                                                                                        {
                                                                                                            let on_move_back = on_move_back_prop.clone();
                                                                                                            let link = row.link.clone();
                                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                                e.prevent_default();
                                                                                                                e.stop_propagation();
                                                                                                                on_move_back.emit(crate::app::MoveBackItem::Row(link.clone()));
                                                                                                            })
                                                                                                        }
                                                                                                    }>
                                                                                                        <Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} />
                                                                                                    </button> } } else { html!{} } }
                                                                                                    {
                                                                                                        if enable_queue_action {
                                                                                                            html!{
                                                                                                                <button class="icon-btn" type_="button" title="Queue" onclick={on_queue_row}>
                                                                                                                    <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                                                                                </button>
                                                                                                            }
                                                                                                        } else {
                                                                                                            html!{}
                                                                                                        }
                                                                                                    }
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
                                    <div class="platform-block" key={platform_key.clone()}>
                                        <div class="platform-item" onclick={on_platform_click}>
                                            <div class="item-left">
                                                <img class="brand-icon" src={platform_icon_src(&plat_label)} />
                                                <span class="item-title">{ plat_label.clone() }</span>
                                            </div>
                                            <div class="item-right">
                                                <span>{ format!("{} collections | {} bookmarks", collections_count, bookmarks_count) }</span>
                                                <button class="icon-btn" type_="button" title="Delete" onclick={on_delete_platform}>
                                                    <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                </button>
                                                        { if !enable_queue_action { html!{ <button class="icon-btn" type_="button" title="Move back to backlog" onclick={on_back_platform}><Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} /></button> } } else { html!{} } }
                                                {
                                                    if enable_queue_action {
                                                        html!{
                                                            <button class="icon-btn" type_="button" title="Queue" onclick={on_queue_platform}>
                                                                <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                            </button>
                                                        }
                                                    } else { html!{} }
                                                }
                                            </div>
                                        </div>
                                        { platform_rows }
                                    </div>
                                }
                            })
                        }
                    </div>
                </>
            }
        }
    };

    html! {
        <main class="container downloads">
            <div style="display:flex; align-items:center; gap:8px; margin: 24px 0 8px 16px;">
                <h2 style="margin:0;">{"Downloading"}</h2>
                <button class="icon-btn" type_="button" onclick={on_toggle_pause_click_header} title={ if props.paused { "Play" } else { "Pause" } }>
                    {
                        if props.paused {
                            html!{ <Icon icon_id={IconId::LucidePlay}  width={"18"} height={"18"} /> }
                        } else {
                            html!{ <Icon icon_id={IconId::LucidePause} width={"18"} height={"18"} /> }
                        }
                    }
                </button>
            </div>

            {
                if !props.active.is_empty() {
                    html!{
                        <div class="summary">
                            <div class="rows-card no-indent">
                                <ul class="rows">
                                    {
                                        for props.active.iter().map(|active| {
                                            let plat_label = platform_str(&active.row.platform).to_string();
                                            html! {
                                                <li class="row-line">
                                                    <img class="brand-icon" src={platform_icon_src(&plat_label)} />
                                                    <span class="link-text">{ collection_title(&active.row) }</span>
                                                    <span class="link-text" style="opacity:0.9;">{" - "}{ item_label_for_row(&active.row) }</span>
                                                    <div class="row-actions">
                                                        <span class="progress-text">{ &active.progress }</span>
                                                    </div>
                                                </li>
                                            }
                                        })
                                    }
                                </ul>
                            </div>
                        </div>
                    }
                } else { html!{} }
            }

            {
                if !props.queue.is_empty() {
                    html!{
                        {
                            render_section(props.queue.clone(), "Queue", false)
                        }
                    }
                } else {
                    html!{}
                }
            }

            { render_section(props.backlog.clone(), "Backlog", true) }
        </main>
    }
}
