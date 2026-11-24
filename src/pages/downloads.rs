use crate::components::service_icon::service_icon;
use crate::app::{DeleteItem, MoveItem};
use crate::dom::assign_missing_descriptive_ids;
use crate::types::{content_type_str, platform_str, ClipRow, ContentType, MediaKind, Platform};
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_icons::{Icon, IconId};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}
#[allow(dead_code)]
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
    pub issues: Vec<ClipRow>,
    pub active: Vec<ActiveDownload>,
    pub loading: bool,
    pub paused: bool,
    pub on_toggle_pause: Callback<()>,
    pub on_delete: Callback<DeleteItem>,
    pub on_move_to_queue: Callback<MoveItem>,
    pub on_move_to_backlog: Callback<crate::app::MoveBackItem>,
    pub on_retry_issue: Callback<i64>,
}

#[derive(Clone, PartialEq)]
pub struct ActiveDownload {
    pub row: ClipRow,
    pub progress: Option<String>,
    pub stage: String,
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
    if (plat == "instagram" && is_instagram_photo(link)) || (plat == "tiktok" && is_tiktok_photo(link)) {
        IconId::LucideImage
    } else {
        match row.media {
            MediaKind::Link => IconId::LucideLink,
            MediaKind::Pictures => IconId::LucideImage,
            MediaKind::Video => IconId::LucideVideo,
        }
    }
}


/* ───────────────────────── component ───────────────────────── */
#[function_component(DownloadsPage)]
pub fn downloads_page(props: &Props) -> Html {
    use_effect(|| {
        assign_missing_descriptive_ids("downloads-page");
        || ()
    });
    let has_any_rows = !props.active.is_empty() || !props.queue.is_empty() || !props.backlog.is_empty() || !props.issues.is_empty();
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());
    // Local overrides so icon flips instantly on click (DB persists separately)
    let output_overrides = use_state(|| std::collections::HashMap::<String, String>::new());

    let on_toggle_pause_click_header = {
        let cb = props.on_toggle_pause.clone();
        Callback::from(move |_e: MouseEvent| cb.emit(()))
    };
    let _on_toggle_pause_click_row = {
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

            // Keep typed keys with their labels so sorting and queue actions use the same groups.
            let mut map: BTreeMap<(String, Platform), BTreeMap<(String, String, ContentType), Vec<ClipRow>>> = BTreeMap::new();

            // De-dupe by (platform, handle, type, link) within this section
            let mut seen = HashSet::<String>::new();

            for mut r in rows_in {
                if r.handle.trim().is_empty() {
                    r.handle = "Unknown".into();
                }
                let plat = platform_str(&r.platform).to_string();
                let typ = content_type_str(&r.content_type).to_string();

                let dedup_key = format!("{}|{}|{}|{}", plat, r.handle.to_lowercase().trim(), typ, r.link.trim());
                if !seen.insert(dedup_key) {
                    continue;
                }

                map.entry((plat, r.platform)).or_default().entry((r.handle.clone(), typ, r.content_type)).or_default().push(r);
            }

            html! {
                <>
                    <h2 id={format!("{section_id}-heading")} style="margin: 24px 0 8px 16px;">{title}</h2>
                    <div id={format!("{section_id}-summary")} class="summary">
                        {
                            for map.into_iter().map(|((plat_label, platform), mut col_map)| {
                                for rows in col_map.values_mut() {
                                    rows.sort_by(|a, b| item_label_for_row(a).cmp(&item_label_for_row(b)));
                                }

                                let collections_count = col_map.len();
                                let bookmarks_count: usize = col_map.values().map(|v| v.len()).sum();

                                /* ---- platform open state (namespaced) ---- */
                                let platform_key = format!("{}::{}", section_id, plat_label);
                                let platform_dom = platform_key.replace("::", "-");
                                let is_open = expanded_platforms.contains(&platform_key);

                                let on_platform_click = {
                                    let expanded_platforms = expanded_platforms.clone();
                                    let k = platform_key.clone();
                                    Callback::from(move |_| {
                                        let mut set = (*expanded_platforms).clone();
                                        if !set.insert(k.clone()) {
                                            set.remove(&k);
                                        }
                                        expanded_platforms.set(set);
                                    })
                                };

                                let on_delete_platform = {
                                    let on_delete = on_delete_prop.clone();
                                    // Backend deletion honoring delete mode
                                    let platform_str_for_backend = plat_label.clone();
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        let platform_str_for_backend = platform_str_for_backend.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({"platform": platform_str_for_backend})).unwrap();
                                            let _ = invoke("delete_rows_by_platform", args).await;
                                        });
                                        on_delete.emit(DeleteItem::Platform(platform.clone()));
                                    })
                                };

                                let on_queue_platform = {
                                    let on_move = on_move_prop.clone();
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        if enable_queue_action {
                                            on_move.emit(MoveItem::Platform(platform));
                                        }
                                    })
                                };

                                let on_back_platform = {
                                    let on_move_back = on_move_back_prop.clone();
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        if !enable_queue_action {
                                            on_move_back.emit(crate::app::MoveBackItem::Platform(platform));
                                        }
                                    })
                                };

                                let platform_rows = if is_open {
                                    html! {
                                        <div id={format!("{platform_dom}-collections")}>
                                            {
                                                for col_map.into_iter().map(|((handle, typ_str, ctype), rows)| {
                                                    /* ---- collection open state (namespaced) ---- */
                                                    let col_key = format!("{}::{}::{}::{}", section_id, plat_label, handle, typ_str);
                                                    let collection_dom = format!("{platform_dom}-collection-{}", crate::dom::hash_fragment(&col_key));
                                                    let col_open = expanded_collections.contains(&col_key);

                                                    let on_col_click = {
                                                        let expanded_collections = expanded_collections.clone();
                                                        let k = col_key.clone();
                                                        Callback::from(move |_| {
                                                            let mut set = (*expanded_collections).clone();
                                                            if !set.insert(k.clone()) {
                                                                set.remove(&k);
                                                            }
                                                            expanded_collections.set(set);
                                                        })
                                                    };

                                                    let on_delete_collection = {
                                                        let on_delete = on_delete_prop.clone();
                                                        let plat = platform;
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
                                                        let handle_s = handle.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            if enable_queue_action {
                                                                on_move.emit(MoveItem::Collection(
                                                                    platform, handle_s.clone(), ctype
                                                                ));
                                                            }
                                                        })
                                                    };

                                                    let on_back_collection = {
                                                        let on_move_back = on_move_back_prop.clone();
                                                        let handle_s = handle.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            if !enable_queue_action {
                                                                on_move_back.emit(crate::app::MoveBackItem::Collection(
                                                                    platform, handle_s.clone(), ctype
                                                                ));
                                                            }
                                                        })
                                                    };

                                                    html!{
                                                        <div id={format!("{collection_dom}-group")} class="collection-block" key={col_key.clone()}>
                                                            <div id={format!("{collection_dom}-toggle")} class="collection-item" onclick={on_col_click}>
                                                                <div id={format!("{collection_dom}-identity")} class="item-left">
                                                                    <span id={format!("{collection_dom}-name")} class="item-title">{format!("{} | {}", handle, typ_str)}</span>
                                                                </div>
                                                                    <div id={format!("{collection_dom}-actions")} class="item-right">
                                                                    <span id={format!("{collection_dom}-count")}>{format!("{} items", rows.len())}</span>
                                                                    <button id={format!("{collection_dom}-delete")} class="icon-btn" type_="button" title="Delete" onclick={on_delete_collection}>
                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                    </button>
                                                                    { if !enable_queue_action {
                                                                        html!{<button id={format!("{collection_dom}-back-to-backlog")} class="icon-btn" type_="button" title="Move back to backlog" onclick={on_back_collection.clone()}><Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} /></button>}
                                                                    } else {
                                                                        html!{}
                                                                    } }
                                                                    {
                                                                        if enable_queue_action {
                                                                            html!{
                                                                                <button id={format!("{collection_dom}-queue")} class="icon-btn" type_="button" title="Queue" onclick={on_queue_collection.clone()}>
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
                                                                        <div id={format!("{collection_dom}-items")} class="rows-card">
                                                                            <ul id={format!("{collection_dom}-list")} class="rows">
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
                                                                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({"link": link_for_backend})).unwrap();
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
                                                                                                if enable_queue_action {
                                                                                                    on_move.emit(MoveItem::Row(link.clone()));
                                                                                                }
                                                                                            })
                                                                                        };
                                                                                        // Determine current effective format (override -> row value)
                                                                                        let effective_fmt: Option<String> = (*output_overrides).get(&row.link).cloned().or_else(|| row.output_format.clone());
                                                                                        let fixed_media = row.media == MediaKind::Link || (row.platform == Platform::Patreon && row.media == MediaKind::Pictures);
                                                                                        let row_icon = if !fixed_media && effective_fmt.as_deref() == Some("audio") {
                                                                                            IconId::LucideMusic
                                                                                        } else {
                                                                                            icon_for_row(&row)
                                                                                        };
                                                                                        let on_click_toggle = {
                                                                                            let link_for_backend = row.link.clone();
                                                                                            let output_overrides = output_overrides.clone();
                                                                                            let current_fmt = effective_fmt.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                // Flip locally first for instant UI feedback
                                                                                                let mut map = (*output_overrides).clone();
                                                                                                let next = if current_fmt.as_deref() == Some("audio") {
                                                                                                    "video"
                                                                                                } else {
                                                                                                    "audio"
                                                                                                };
                                                                                                map.insert(link_for_backend.clone(), next.to_string());
                                                                                                output_overrides.set(map);
                                                                                                // Persist to DB
                                                                                                let link_for_backend = link_for_backend.clone();
                                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({"link": link_for_backend})).unwrap();
                                                                                                    let _ = invoke("toggle_output_format", args).await;
                                                                                                });
                                                                                            })
                                                                                        };
                                                                                        html!{
                                                                                            <li id={format!("downloads-item-{}-item", row.id)} class="row-line" key={row.link.clone()}>
                                                                                                <span id={format!("downloads-item-{}-media-type", row.id)} title={if row.media == MediaKind::Link { "Link" } else { "" }} onclick={(!fixed_media).then_some(on_click_toggle.clone())}><Icon icon_id={row_icon} width={"16"} height={"16"} /></span>
                                                                                                <a id={format!("downloads-item-{}-source-link", row.id)} class="link-text" href={row.link.clone()} target="_blank">
                                                                                                    {item_label_for_row(&row)}
                                                                                                </a>
                                                                                                <div id={format!("downloads-item-{}-actions", row.id)} class="row-actions">
                                                                                                    <button id={format!("downloads-item-{}-delete", row.id)} class="icon-btn" type_="button" title="Delete" onclick={on_delete_row}>
                                                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                                                    </button>
                                                                                                    { if !enable_queue_action { html!{ <button id={format!("downloads-item-{}-back-to-backlog", row.id)} class="icon-btn" type_="button" title="Move back to backlog" onclick={
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
                                                                                                    </button> } } else {
                                                                                                        html!{}
                                                                                                    } }
                                                                                                    {
                                                                                                        if enable_queue_action {
                                                                                                            html!{
                                                                                                                <button id={format!("downloads-item-{}-queue", row.id)} class="icon-btn" type_="button" title="Queue" onclick={on_queue_row}>
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
                                                                } else {
                                                                    html!{}
                                                                }
                                                            }
                                                        </div>
                                                    }
                                                })
                                            }
                                        </div>
                                    }
                                } else {
                                    html!{}
                                };

                                html! {
                                    <div id={format!("{platform_dom}-group")} class="platform-block" key={platform_key.clone()}>
                                        <div id={format!("{platform_dom}-toggle")} class="platform-item" onclick={on_platform_click}>
                                            <div id={format!("{platform_dom}-identity")} class="item-left">
                                                {service_icon(format!("{platform_dom}-icon"), &plat_label)}
                                                <span id={format!("{platform_dom}-name")} class="item-title">{plat_label.clone()}</span>
                                            </div>
                                            <div id={format!("{platform_dom}-actions")} class="item-right">
                                                <span id={format!("{platform_dom}-count")}>{format!("{} collections | {} items", collections_count, bookmarks_count)}</span>
                                                <button id={format!("{platform_dom}-delete")} class="icon-btn" type_="button" title="Delete" onclick={on_delete_platform}>
                                                    <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                </button>
                                                        { if !enable_queue_action {
                                                            html!{<button id={format!("{platform_dom}-back-to-backlog")} class="icon-btn" type_="button" title="Move back to backlog" onclick={on_back_platform}><Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} /></button>}
                                                        } else {
                                                            html!{}
                                                        } }
                                                {
                                                    if enable_queue_action {
                                                        html!{
                                                            <button id={format!("{platform_dom}-queue")} class="icon-btn" type_="button" title="Queue" onclick={on_queue_platform}>
                                                                <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                            </button>
                                                        }
                                                    } else {
                                                        html!{}
                                                    }
                                                }
                                            </div>
                                        </div>
                                        {platform_rows}
                                    </div>
                                }
                            })
                        }
                    </div>
                </>
            }
        }
    };

    let render_issues = {
        let expanded_platforms = expanded_platforms.clone();
        let expanded_collections = expanded_collections.clone();
        let on_delete_prop = props.on_delete.clone();
        let on_move_back_prop = props.on_move_to_backlog.clone();
        let on_retry_prop = props.on_retry_issue.clone();

        move |rows_in: Vec<ClipRow>| -> Html {
            use std::collections::BTreeMap;

            let mut map: BTreeMap<(String, Platform), BTreeMap<(String, String, ContentType), Vec<ClipRow>>> = BTreeMap::new();
            for mut r in rows_in {
                if r.handle.trim().is_empty() {
                    r.handle = "Unknown".into();
                }
                let plat = platform_str(&r.platform).to_string();
                let typ = content_type_str(&r.content_type).to_string();
                map.entry((plat, r.platform)).or_default().entry((r.handle.clone(), typ, r.content_type)).or_default().push(r);
            }

            html! {
                <>
                    <h2 id="issues-heading" style="margin: 24px 0 8px 16px;">{"Issues"}</h2>
                    <div id="issues-summary" class="summary">
                        {
                            for map.into_iter().map(|((plat_label, platform), col_map)| {
                                let total: usize = col_map.values().map(|v| v.len()).sum();
                                let col_count = col_map.len();
                                let platform_key = format!("issues::{}", plat_label);
                                let platform_dom = platform_key.replace("::", "-");
                                let is_open = expanded_platforms.contains(&platform_key);

                                let on_platform_click = {
                                    let expanded_platforms = expanded_platforms.clone();
                                    let k = platform_key.clone();
                                    Callback::from(move |_| {
                                        let mut set = (*expanded_platforms).clone();
                                        if !set.insert(k.clone()) {
                                            set.remove(&k);
                                        }
                                        expanded_platforms.set(set);
                                    })
                                };

                                let on_delete_platform = {
                                    let on_delete = on_delete_prop.clone();
                                    let plat_s = plat_label.clone();
                                    Callback::from(move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        let plat_s = plat_s.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({"platform": plat_s})).unwrap();
                                            let _ = invoke("delete_rows_by_platform", args).await;
                                        });
                                        on_delete.emit(DeleteItem::Platform(platform));
                                    })
                                };

                                let platform_rows = if is_open {
                                    html! {
                                        <div id={format!("{platform_dom}-collections")}>
                                            {
                                                for col_map.into_iter().map(|((handle, typ_str, ctype), rows)| {
                                                    let col_key = format!("issues::{}::{}::{}", plat_label, handle, typ_str);
                                                    let collection_dom = format!("{platform_dom}-collection-{}", crate::dom::hash_fragment(&col_key));
                                                    let col_open = expanded_collections.contains(&col_key);

                                                    let on_col_click = {
                                                        let expanded_collections = expanded_collections.clone();
                                                        let k = col_key.clone();
                                                        Callback::from(move |_| {
                                                            let mut set = (*expanded_collections).clone();
                                                            if !set.insert(k.clone()) {
                                                                set.remove(&k);
                                                            }
                                                            expanded_collections.set(set);
                                                        })
                                                    };

                                                    let on_delete_col = {
                                                        let on_delete = on_delete_prop.clone();
                                                        let plat_s = plat_label.clone();
                                                        let handle_s = handle.clone();
                                                        let typ_s = typ_str.clone();
                                                        Callback::from(move |e: MouseEvent| {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            let plat_s = plat_s.clone();
                                                            let handle_s = handle_s.clone();
                                                            let handle_b = handle_s.clone();
                                                            let typ_s = typ_s.clone();
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                                    "platform": plat_s,
                                                                    "handle": handle_b,
                                                                    "origin": typ_s,
                                                                })).unwrap();
                                                                let _ = invoke("delete_rows_by_collection", args).await;
                                                            });
                                                            on_delete.emit(DeleteItem::Collection(platform, handle_s, ctype));
                                                        })
                                                    };

                                                    html! {
                                                        <div id={format!("{collection_dom}-group")} class="collection-block" key={col_key.clone()}>
                                                            <div id={format!("{collection_dom}-toggle")} class="collection-item" onclick={on_col_click}>
                                                                <div id={format!("{collection_dom}-identity")} class="item-left">
                                                                    <span id={format!("{collection_dom}-name")} class="item-title">{format!("{} | {}", handle, typ_str)}</span>
                                                                </div>
                                                                <div id={format!("{collection_dom}-actions")} class="item-right">
                                                                    <span id={format!("{collection_dom}-count")}>{format!("{} items", rows.len())}</span>
                                                                    <button id={format!("{collection_dom}-delete")} class="icon-btn" type_="button" title="Delete" onclick={on_delete_col}>
                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                    </button>
                                                                </div>
                                                            </div>
                                                            {
                                                                if col_open {
                                                                    html! {
                                                                        <div id={format!("{collection_dom}-items")} class="rows-card">
                                                                            <ul id={format!("{collection_dom}-list")} class="rows">
                                                                                {
                                                                                    for rows.into_iter().map(|row| {
                                                                                        let issue_id = row.id;
                                                                                        let on_delete_row = {
                                                                                            let on_delete = on_delete_prop.clone();
                                                                                            let link = row.link.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                let link_b = link.clone();
                                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({"link": link_b})).unwrap();
                                                                                                    let _ = invoke("delete_rows_by_link", args).await;
                                                                                                });
                                                                                                on_delete.emit(DeleteItem::Row(link.clone()));
                                                                                            })
                                                                                        };
                                                                                        let on_backlog_row = {
                                                                                            let on_move_back = on_move_back_prop.clone();
                                                                                            let link = row.link.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                on_move_back.emit(crate::app::MoveBackItem::Row(link.clone()));
                                                                                            })
                                                                                        };
                                                                                        let on_retry_row = {
                                                                                            let on_retry = on_retry_prop.clone();
                                                                                            Callback::from(move |e: MouseEvent| {
                                                                                                e.prevent_default();
                                                                                                e.stop_propagation();
                                                                                                on_retry.emit(issue_id);
                                                                                            })
                                                                                        };
                                                                                        html! {
                                                                                            <li id={format!("downloads-item-{}-item", row.id)} class="row-line issue-line" key={row.link.clone()}>
                                                                                                <div id={format!("downloads-item-{}-details", row.id)} class="issue-copy">
                                                                                                    <div id={format!("downloads-item-{}-title", row.id)} class="issue-title">
                                                                                                        <span id={format!("downloads-item-{}-label", row.id)} class="link-text">{item_label_for_row(&row)}</span>
                                                                                                    </div>
                                                                                                    <div id={format!("downloads-item-{}-reason", row.id)} class="issue-reason">
                                                                                                        {row.last_error.clone().unwrap_or_else(|| "Download failed".into())}
                                                                                                    </div>
                                                                                                </div>
                                                                                                <div id={format!("downloads-item-{}-actions", row.id)} class="row-actions active-status issue-actions">
                                                                                                    <button id={format!("downloads-item-{}-delete", row.id)} class="icon-btn" type_="button" title="Delete" onclick={on_delete_row}>
                                                                                                        <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                                                                    </button>
                                                                                                    <button id={format!("downloads-item-{}-back-to-backlog", row.id)} class="icon-btn" type_="button" title="Move back to backlog" onclick={on_backlog_row}>
                                                                                                        <Icon icon_id={IconId::LucideRotateCcw} width={"18"} height={"18"} />
                                                                                                    </button>
                                                                                                    <button id={format!("downloads-item-{}-retry", row.id)} class="icon-btn" type_="button" title="Retry" onclick={on_retry_row}>
                                                                                                        <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                                                                    </button>
                                                                                                </div>
                                                                                            </li>
                                                                                        }
                                                                                    })
                                                                                }
                                                                            </ul>
                                                                        </div>
                                                                    }
                                                                } else {
                                                                    html!{}
                                                                }
                                                            }
                                                        </div>
                                                    }
                                                })
                                            }
                                        </div>
                                    }
                                } else {
                                    html!{}
                                };

                                html! {
                                    <div id={format!("{platform_dom}-group")} class="platform-block" key={platform_key.clone()}>
                                        <div id={format!("{platform_dom}-toggle")} class="platform-item" onclick={on_platform_click}>
                                            <div id={format!("{platform_dom}-identity")} class="item-left">
                                                {service_icon(format!("{platform_dom}-icon"), &plat_label)}
                                                <span id={format!("{platform_dom}-name")} class="item-title">{plat_label.clone()}</span>
                                            </div>
                                            <div id={format!("{platform_dom}-actions")} class="item-right">
                                                <span id={format!("{platform_dom}-count")}>{format!("{} collections | {} items", col_count, total)}</span>
                                                <button id={format!("{platform_dom}-delete")} class="icon-btn" type_="button" title="Delete" onclick={on_delete_platform}>
                                                    <Icon icon_id={IconId::LucideTrash2} width={"18"} height={"18"} />
                                                </button>
                                            </div>
                                        </div>
                                        {platform_rows}
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
        <main id="downloads-page" class="container downloads">
            <div id="downloads-active-header" style="display:flex; align-items:center; gap:8px; margin: 24px 0 8px 16px;">
                <h2 id="downloads-heading" style="margin:0;">{"Downloading"}</h2>
                <button id="downloads-toggle-pause" class="icon-btn" type_="button" onclick={on_toggle_pause_click_header} title={ if props.paused {
                    "Play"
                } else {
                    "Pause"
                } }>
                    {
                        if props.paused {
                            html!{<Icon icon_id={IconId::LucidePlay}  width={"18"} height={"18"} />}
                        } else {
                            html!{<Icon icon_id={IconId::LucidePause} width={"18"} height={"18"} />}
                        }
                    }
                </button>
            </div>

            {
                if props.loading && !has_any_rows {
                    html! {
                        <div id="downloads-loading-summary" class="summary">
                            <p id="downloads-loading-message" style="margin: 0 0 8px 16px; opacity: 0.85;">{"Loading downloads..."}</p>
                        </div>
                    }
                } else {
                    html! {}
                }
            }

            {
                if !props.active.is_empty() {
                    html!{
                        <div id="downloads-summary" class="summary">
                            <div id="downloads-active-card" class="rows-card no-indent">
                                <ul id="downloads-active-list" class="rows">
                                    {
                                        for props.active.iter().map(|active| {
                                            let plat_label = platform_str(&active.row.platform).to_string();
                                            html! {
                                                <li id={format!("downloads-item-{}-item", active.row.id)} class="row-line">
                                                    {service_icon(format!("downloads-item-{}-icon", active.row.id), &plat_label)}
                                                    <span id={format!("downloads-item-{}-collection", active.row.id)} class="link-text">{collection_title(&active.row)}</span>
                                                    <span id={format!("downloads-item-{}-label", active.row.id)} class="link-text" style="opacity:0.9;">{" - "}{item_label_for_row(&active.row)}</span>
                                                    <div id={format!("downloads-item-{}-actions", active.row.id)} class="row-actions active-status">
                                                        <span id={format!("downloads-item-{}-stage", active.row.id)} class="stage-text">{&active.stage}</span>
                                                        {
                                                            if let Some(progress) = &active.progress {
                                                                html! {<span id={format!("downloads-item-{}-progress", active.row.id)} class="progress-text">{progress}</span>}
                                                            } else {
                                                                html! {}
                                                            }
                                                        }
                                                    </div>
                                                </li>
                                            }
                                        })
                                    }
                                </ul>
                            </div>
                        </div>
                    }
                } else {
                    html!{}
                }
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

            {
                if !props.backlog.is_empty() {
                    html! {render_section(props.backlog.clone(), "Backlog", true)}
                } else {
                    html! {}
                }
            }

            {
                if !props.issues.is_empty() {
                    render_issues(props.issues.clone())
                } else {
                    html! {}
                }
            }
        </main>
    }
}
