use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
use crate::pages; // declared in main.rs
use crate::types::{Platform, ContentType, MediaKind, ClipRow, platform_str, content_type_str};
use yew::prelude::*;
use lucide_yew::Repeat2;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> {
    name: &'a str,
}

// Types moved to crate::types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Downloads,
    Library,
    Settings,
}

// ClipRow imported from crate::types

fn parse_csv(csv_text: &str) -> Vec<ClipRow> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .has_headers(true)
        .from_reader(csv_text.as_bytes());

    let mut rows: Vec<ClipRow> = Vec::new();
    for record in reader.deserialize::<ClipRow>() {
        match record {
            Ok(row) => rows.push(row),
            Err(err) => web_sys::console::error_1(&format!("CSV parse error: {err}").into()),
        }
    }
    rows
}

// helpers imported from crate::types

fn display_label_for_row(row: &ClipRow) -> String {
    // For instagram links, prefer username - kind - id when present; else trim to last two segments
    // Examples:
    // https://www.instagram.com/lucamaxiim/reel/DO_2VI0D-gv/ -> "lucamaxiim - reel - DO_2VI0D-gv"
    // https://www.instagram.com/p/Cr3YFovAh4R/ -> "p/Cr3YFovAh4R"
    let url = row.link.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() >= 6 && (parts[3] == "www.instagram.com" || parts[2].contains("instagram.com")) {
        // parts: [https:, , www.instagram.com, <user_or_p>, <type_or_id>, <id_opt>]
        if parts.len() >= 6 && parts[4] != "p" {
            // username flow: /<username>/<kind>/<id>
            let username = parts[4];
            let kind = parts.get(5).unwrap_or(&"");
            let id = parts.get(6).unwrap_or(&"");
            let id = if id.is_empty() { parts.last().unwrap_or(&"") } else { id };
            // Avoid duplicate segments: if kind == id, show only one
            if kind == id { return format!("{} - {}", username, kind); }
            return format!("{} - {} - {}", username, kind, id);
        }
    }
    // Fallback: last two segments
    if parts.len() >= 2 {
        let a = parts[parts.len()-2];
        let b = parts[parts.len()-1];
        if a == b { return a.to_string(); }
        return format!("{}/{}", a, b);
    }
    row.link.clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CollectionKey {
    platform: Platform,
    handle: String,
    content_type: ContentType,
}

#[derive(Debug, Clone)]
struct CollectionSummary {
    key: CollectionKey,
    rows: Vec<ClipRow>,
}

#[derive(Debug, Clone)]
struct PlatformSummary {
    platform: Platform,
    collections: Vec<CollectionSummary>,
}

fn group_by_platform_and_collection(rows: &[ClipRow]) -> Vec<PlatformSummary> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, BTreeMap<(String, String), Vec<ClipRow>>> = BTreeMap::new();

    for r in rows.iter().cloned() {
        let plat = platform_str(&r.platform).to_string();
        let typ = content_type_str(&r.content_type).to_string();
        let handle = r.handle.clone();
        map.entry(plat)
            .or_default()
            .entry((handle, typ))
            .or_default()
            .push(r);
    }

    let mut result = Vec::new();
    for (plat_str, col_map) in map.into_iter() {
        let platform = match plat_str.as_str() {
            "tiktok" => Platform::Tiktok,
            _ => Platform::Instagram,
        };
        let mut collections = Vec::new();
        for ((handle, typ_str), rows) in col_map.into_iter() {
            let content_type = match typ_str.as_str() {
                "liked" => ContentType::Liked,
                "profile" => ContentType::Profile,
                "bookmarks" => ContentType::Bookmarks,
                _ => ContentType::Reposts,
            };
            collections.push(CollectionSummary {
                key: CollectionKey { platform: platform.clone(), handle, content_type },
                rows,
            });
        }
        result.push(PlatformSummary { platform, collections });
    }
    result
}

#[function_component(App)]
pub fn app() -> Html {
    let greet_input_ref = use_node_ref();

    let name = use_state(|| String::new());
    let page = use_state(|| Page::Home);
    // Downloads page manages its own expand state

    let greet_msg = use_state(|| String::new());
    let queue_rows = use_state(|| Vec::<ClipRow>::new());
    // removed greet side-effect

    let greet = {
        let name = name.clone();
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let value = greet_input_ref.cast::<web_sys::HtmlInputElement>().unwrap().value();
            name.set(value.clone());
            // trigger download via tauri backend
            wasm_bindgen_futures::spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "url": value })).unwrap();
                let _ = invoke("download_url", args).await;
            });
        })
    };

    let open_file_click = {
        let queue_rows = queue_rows.clone();
        Callback::from(move |_: MouseEvent| {
            let q = queue_rows.clone();
            spawn_local(async move {
                // Use Tauri dialog to start at the home directory
                let csv_js = invoke("pick_csv_and_read", JsValue::NULL).await;
                if let Some(csv_text) = csv_js.as_string() {
                    let rows = parse_csv(&csv_text);
                    q.set(rows);
                }
            });
        })
    };

    // Callback for HomePage Import list button (takes unit)
    let on_open_file = {
        let queue_rows = queue_rows.clone();
        Callback::from(move |_: ()| {
            let q = queue_rows.clone();
            spawn_local(async move {
                let csv_js = invoke("pick_csv_and_read", JsValue::NULL).await;
                if let Some(csv_text) = csv_js.as_string() {
                    let rows = parse_csv(&csv_text);
                    q.set(rows);
                }
            });
        })
    };

    // Removed: HTML input file flow in favor of Tauri dialog

    let set_page = |p: Page, page: UseStateHandle<Page>| Callback::from(move |_| page.set(p));

    let sidebar = {
        let page = page.clone();
        html! {
            <aside class="sidebar">
                <button class="nav-btn" onclick={set_page(Page::Home, page.clone())} title="Home">
                    <Icon icon_id={IconId::LucideHome} width={"28"} height={"28"} />
                </button>
                <button class="nav-btn" onclick={set_page(Page::Downloads, page.clone())} title="Downloads">
                    <Icon icon_id={IconId::LucideDownload} width={"28"} height={"28"} />
                </button>
                <button class="nav-btn" onclick={set_page(Page::Library, page.clone())} title="Library">
                    <Icon icon_id={IconId::LucideLibrary} width={"28"} height={"28"} />
                </button>
                <button class="nav-btn" onclick={set_page(Page::Settings, page.clone())} title="Settings">
                    <Icon icon_id={IconId::LucideSettings} width={"28"} height={"28"} />
                </button>
            </aside>
        }
    };

    let body = match *page {
        Page::Home => html! { <pages::home::HomePage on_open_file={on_open_file} /> },
        Page::Downloads => html! { <pages::downloads::DownloadsPage rows={(*queue_rows).clone()} /> },
        Page::Library => html! { <pages::library::LibraryPage /> },
        Page::Settings => html! { <pages::settings::SettingsPage /> },
    };

    html! {
        <>
            { sidebar }
            { body }
        </>
    }
}
