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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Tiktok,
    Instagram,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[function_component(App)]
pub fn app() -> Html {
    let greet_input_ref = use_node_ref();

    let name = use_state(|| String::new());
    let page = use_state(|| Page::Home);

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
        Page::Downloads => html! {
            <main class="container" style="padding-top: 10vh;">
                <table class="queue-table">
                    <thead>
                        <tr>
                            <th>{"Platform"}</th>
                            <th>{"Type"}</th>
                            <th>{"Handle"}</th>
                            <th>{"Media"}</th>
                            <th>{"Link"}</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for queue_rows.iter().map(|row| html!{
                            <tr>
                                <td>{ platform_str(&row.platform) }</td>
                                <td>
                                    {
                                        match row.content_type {
                                            ContentType::Bookmarks => html!{ <Icon icon_id={IconId::LucideBookmark} width={"20"} height={"20"} /> },
                                            ContentType::Liked => html!{ <Icon icon_id={IconId::LucideHeart} width={"20"} height={"20"} /> },
                                            ContentType::Profile => html!{ <Icon icon_id={IconId::LucideUser} width={"20"} height={"20"} /> },
                                            ContentType::Reposts => html!{ <Repeat2 size=20 /> },
                                        }
                                    }
                                </td>
                                <td>{ row.handle.clone() }</td>
                                <td>
                                    {
                                        match row.media {
                                            MediaKind::Pictures => html!{ <Icon icon_id={IconId::LucideImage} width={"20"} height={"20"} /> },
                                            MediaKind::Video => html!{ <Icon icon_id={IconId::LucideVideo} width={"20"} height={"20"} /> },
                                        }
                                    }
                                </td>
                                <td>
                                    <a href={row.link.clone()} target="_blank">{ row.link.clone() }</a>
                                </td>
                            </tr>
                        })}
                    </tbody>
                </table>
            </main>
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
