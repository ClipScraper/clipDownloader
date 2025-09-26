use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
use crate::pages; // declared in main.rs
use crate::types::{ClipRow};
use yew::prelude::*;

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

#[function_component(App)]
pub fn app() -> Html {
    let greet_input_ref = use_node_ref();

    let name = use_state(|| String::new());
    let page = use_state(|| Page::Home);
    // Downloads page manages its own expand state
    let queue_rows = use_state(|| Vec::<ClipRow>::new());

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
