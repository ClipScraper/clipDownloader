use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_icons::{Icon, IconId};
use crate::types::{ClipRow, MediaKind, platform_str, content_type_str};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

/* ───────── helpers mirrored from downloads.rs for consistent look ───────── */

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
        _ => format!("{}/{}", parts[parts.len()-2], parts[parts.len()-1]),
    }
}
fn item_label_for_row(row: &ClipRow) -> String {
    last_two_path_segments(&row.link)
}
fn platform_icon_src(p: &str) -> &'static str {
    match p {
        "instagram" => "public/instagram.webp",
        "tiktok"    => "public/tiktok.webp",
        "youtube"   => "public/youtube.webp",
        _           => "",
    }
}
fn collection_title(row: &ClipRow) -> String {
    let handle = if row.handle.trim().is_empty() { "Unknown" } else { &row.handle };
    let typ = content_type_str(&row.content_type);
    format!("{handle} | {typ}")
}

/* ───────────────────────── component ───────────────────────── */

#[function_component(LibraryPage)]
pub fn library_page() -> Html {
    let done_rows = use_state(|| Vec::<ClipRow>::new());

    // load once
    {
        let done_rows = done_rows.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let v = invoke("list_done", JsValue::NULL).await;
                if let Ok(rows) = serde_wasm_bindgen::from_value::<Vec<ClipRow>>(v) {
                    done_rows.set(rows);
                }
            });
            || ()
        });
    }

    // expand/collapse state (namespaced with "library")
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());

    // group like /downloads: platform -> (handle,type) -> rows
    use std::collections::{BTreeMap, HashSet};
    let mut map: BTreeMap<String, BTreeMap<(String, String), Vec<ClipRow>>> = BTreeMap::new();
    let mut seen = HashSet::<String>::new();

    for mut r in (*done_rows).clone() {
        if r.handle.trim().is_empty() { r.handle = "Unknown".into(); }
        let plat = platform_str(&r.platform).to_string();
        let typ  = content_type_str(&r.content_type).to_string();

        let key = format!("{}|{}|{}|{}", plat, r.handle.to_lowercase().trim(), typ, r.link.trim());
        if !seen.insert(key) { continue; }

        map.entry(plat)
            .or_default()
            .entry((r.handle.clone(), typ))
            .or_default()
            .push(r);
    }

    // sort each collection by label for stability
    for col_map in map.values_mut() {
        for rows in col_map.values_mut() {
            rows.sort_by(|a, b| item_label_for_row(a).cmp(&item_label_for_row(b)));
        }
    }

    html! {
        <main class="container">
            <h1>{"Library"}</h1>
            <div class="summary">
                {
                    for map.into_iter().map(|(plat_label, col_map)| {
                        let section_id = "library";
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

                        let collections_count = col_map.len();
                        let items_count: usize = col_map.values().map(|v| v.len()).sum();

                        let platform_rows = if is_open {
                            html!{
                                <div>
                                    {
                                        for col_map.into_iter().map(|((handle, typ_str), rows)| {
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
                                            html!{
                                                <div class="collection-block" key={col_key.clone()}>
                                                    <div class="collection-item" onclick={on_col_click}>
                                                        <div class="item-left">
                                                            <span class="item-title">{ format!("{} | {}", handle, typ_str) }</span>
                                                        </div>
                                                        <div class="item-right">
                                                            <span>{ format!("{} items", rows.len()) }</span>
                                                        </div>
                                                    </div>
                                                    {
                                                        if col_open {
                                                            html!{
                                                                <div class="rows-card">
                                                                    <ul class="rows">
                                                                        {
                                                                            for rows.into_iter().map(|row| {
                                                                                html!{
                                                                                    <li class="row-line" key={row.link.clone()}>
                                                                                        {
                                                                                            match row.media {
                                                                                                MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"16"} height={"16"} /> },
                                                                                                MediaKind::Video    => html!{ <Icon icon_id={IconId::LucideVideo} width={"16"} height={"16"} /> },
                                                                                            }
                                                                                        }
                                                                                        <a class="link-text" href={row.link.clone()} target="_blank">
                                                                                            { collection_title(&row) }{" - "}{ item_label_for_row(&row) }
                                                                                        </a>
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

                        html!{
                            <div class="platform-block" key={platform_key.clone()}>
                                <div class="platform-item" onclick={on_platform_click}>
                                    <div class="item-left">
                                        <img class="brand-icon" src={platform_icon_src(&plat_label)} />
                                        <span class="item-title">{ plat_label.clone() }</span>
                                    </div>
                                    <div class="item-right">
                                        <span>{ format!("{} collections | {} items", collections_count, items_count) }</span>
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
