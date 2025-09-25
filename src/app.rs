use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Tiktok,
    Instagram,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
enum ContentType {
    Liked,
    Reposts,
    Profile,
    Bookmarks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MediaKind {
    Pictures,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Downloads,
    Library,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ClipRow {
    #[serde(rename = "Platform")]
    platform: Platform,
    #[serde(rename = "Type")]
    content_type: ContentType,
    #[serde(rename = "Handle")]
    handle: String,
    #[serde(rename = "Media")]
    media: MediaKind,
    #[serde(rename = "link")]
    link: String,
}

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

fn platform_str(p: &Platform) -> &'static str {
    match p { Platform::Tiktok => "tiktok", Platform::Instagram => "instagram" }
}
fn content_type_str(t: &ContentType) -> &'static str {
    match t {
        ContentType::Liked => "liked",
        ContentType::Reposts => "reposts",
        ContentType::Profile => "profile",
        ContentType::Bookmarks => "bookmarks",
    }
}
fn media_str(m: &MediaKind) -> &'static str {
    match m { MediaKind::Pictures => "pictures", MediaKind::Video => "video" }
}

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
            return format!("{} - {} - {}", username, kind, id);
        }
    }
    // Fallback: last two segments
    if parts.len() >= 2 {
        return format!("{}/{}", parts[parts.len()-2], parts[parts.len()-1]);
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
    let expanded_platforms = use_state(|| std::collections::HashSet::<String>::new());
    let expanded_collections = use_state(|| std::collections::HashSet::<String>::new());

    let greet_msg = use_state(|| String::new());
    let queue_rows = use_state(|| Vec::<ClipRow>::new());
    {
        let greet_msg = greet_msg.clone();
        let name = name.clone();
        let name2 = name.clone();
        use_effect_with(
            name2,
            move |_| {
                spawn_local(async move {
                    if name.is_empty() {
                        return;
                    }

                    let args = serde_wasm_bindgen::to_value(&GreetArgs { name: &*name }).unwrap();
                    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
                    let new_msg = invoke("greet", args).await.as_string().unwrap();
                    greet_msg.set(new_msg);
                });

                || {}
            },
        );
    }

    let greet = {
        let name = name.clone();
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            name.set(
                greet_input_ref
                    .cast::<web_sys::HtmlInputElement>()
                    .unwrap()
                    .value(),
            );
        })
    };

    let open_file_click = {
        let queue_rows = queue_rows.clone();
        Callback::from(move |_| {
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
        Page::Home => html! {
            <main class="container">
                <h1>{"Welcome to Clip Downloader"}</h1>
                <form class="row" onsubmit={greet}>
                    <input id="greet-input" ref={greet_input_ref} placeholder="Enter url..." />
                    <button type="submit">{"Download"}</button>
                </form>
                <p>{ &*greet_msg }</p>
                <div class="row">
                    <button type="button" onclick={open_file_click}>{"Open file"}</button>
                </div>
            </main>
        },
        Page::Downloads => {
            let platform_summaries = group_by_platform_and_collection(&queue_rows);

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
                        { for platform_summaries.into_iter().map(|ps| {
                            let plat_label = platform_str(&ps.platform).to_string();
                            let collections_count = ps.collections.len();
                            let bookmarks_count: usize = ps.collections.iter().map(|c| c.rows.len()).sum();
                            let key = plat_label.clone();
                            let is_open = expanded_platforms.contains(&key);
                            let on_click = {
                                let toggle_platform = toggle_platform.clone();
                                let k = key.clone();
                                Callback::from(move |_| toggle_platform.emit(k.clone()))
                            };

                            let collections_html = if is_open {
                                html! {
                                    <div>
                                        { for ps.collections.into_iter().map(|c| {
                                            let col_key = format!("{}::{}::{}", plat_label, c.key.handle, content_type_str(&c.key.content_type));
                                            let col_open = expanded_collections.contains(&col_key);
                                            let on_col_click = {
                                                let toggle_collection = toggle_collection.clone();
                                                let k = col_key.clone();
                                                Callback::from(move |_| toggle_collection.emit(k.clone()))
                                            };
                                            html! {
                                                <div>
                                                    <div class="collection-item" onclick={on_col_click}>
                                                        <div class="item-left">
                                                            <span>{ format!("{} | {}", c.key.handle, content_type_str(&c.key.content_type)) }</span>
                                                        </div>
                                                        <div class="item-right">
                                                            <span>{ format!("{} bookmarks", c.rows.len()) }</span>
                                                            <button class="icon-btn" title="Download">
                                                                <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                                            </button>
                                                        </div>
                                                    </div>
                                                    { if col_open {
                                                        html! {
                                                            <ul class="rows">
                                                                { for c.rows.into_iter().map(|row| html!{
                                                                    <li>
                                                                        {
                                                                            match row.media {
                                                                                MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"16"} height={"16"} /> },
                                                                                MediaKind::Video => html!{ <Icon icon_id={IconId::LucideVideo} width={"16"} height={"16"} /> },
                                                                            }
                                                                        }
                                                                        <a href={row.link.clone()} target="_blank">{ display_label_for_row(&row) }</a>
                                                                        <button class="icon-btn" title="Download">
                                                                            <Icon icon_id={IconId::LucideDownload} width={"16"} height={"16"} />
                                                                        </button>
                                                                    </li>
                                                                }) }
                                                            </ul>
                                                        }
                                                    } else { html!{} } }
                                                </div>
                                            }
                                        }) }
                                    </div>
                                }
                            } else { html!{} };

                            html! {
                                <div>
                                    <div class="platform-item" onclick={on_click}>
                                        <div class="item-left">
                                            <img class="brand-icon" src={if plat_label == "instagram" { "public/instagram.webp" } else { "public/tiktok.webp" }} />
                                            <span>{ plat_label.clone() }</span>
                                        </div>
                                        <div class="item-right">
                                            <span>{ format!("{} collections | {} bookmarks", collections_count, bookmarks_count) }</span>
                                            <button class="icon-btn" title="Download">
                                                <Icon icon_id={IconId::LucideDownload} width={"18"} height={"18"} />
                                            </button>
                                        </div>
                                    </div>
                                    { collections_html }
                                </div>
                            }
                        }) }
                    </div>
                </main>
            }
        },
        Page::Library => html! {
            <main class="container" style="padding-top: 20vh;">
                <Icon icon_id={IconId::LucideLibrary} width={"64"} height={"64"} />
            </main>
        },
        Page::Settings => html! {
            <main class="container" style="padding-top: 20vh;">
                <Icon icon_id={IconId::LucideSettings} width={"64"} height={"64"} />
            </main>
        },
    };

    html! {
        <>
            { sidebar }
            { body }
        </>
    }
}
