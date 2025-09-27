// ===== src/app.rs =====
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
use crate::pages; // declared in main.rs
use crate::types::ClipRow;
use yew::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Tauri v2 JS bridges (NO nesting of extern blocks)

// Tauri core.invoke returns a Promise -> mark async so we can .await it.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

// Tauri event.listen returns a Promise<UnlistenFn> -> async is fine.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, f: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

// Webview helper: returns a Webview object synchronously.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window","__TAURI__","webview"])]
    fn getCurrentWebview() -> JsValue;
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> { name: &'a str }

#[derive(Serialize, Deserialize)]
struct ReadCsvFromPathArgs<'a> { path: &'a str }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page { Home, Downloads, Library, Settings }

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

// Pretty-log a JsValue to the console as JSON (best-effort).
fn log_json(label: &str, v: &JsValue) {
    let s = js_sys::JSON::stringify(v)
        .ok()
        .and_then(|j| j.as_string())
        .unwrap_or_else(|| "<unstringifiable>".to_string());
    web_sys::console::log_2(&JsValue::from_str(label), &JsValue::from_str(&s));
}

// Use the Tauri command to read a file and push it into Downloads.
fn handle_dropped_path(path: String, q: UseStateHandle<Vec<ClipRow>>, page: UseStateHandle<Page>) {
    web_sys::console::log_1(&JsValue::from_str(&format!("➡️ drop: {}", path)));
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&ReadCsvFromPathArgs { path: &path }).unwrap();
        let csv_js = invoke("read_csv_from_path", args).await;
        if let Some(csv_text) = csv_js.as_string() {
            web_sys::console::log_1(&JsValue::from_str(&format!("📄 read_csv_from_path OK, {} bytes", csv_text.len())));
            let rows = parse_csv(&csv_text);
            web_sys::console::log_1(&JsValue::from_str(&format!("✅ parsed {} rows", rows.len())));
            q.set(rows);
            page.set(Page::Downloads);
        } else {
            web_sys::console::error_1(&"❌ read_csv_from_path failed (no string)".into());
        }
    });
}

// Start BOTH listeners: (A) official onDragDropEvent, (B) raw event fallback.
async fn start_dragdrop_listener(q: UseStateHandle<Vec<ClipRow>>, page: UseStateHandle<Page>) {
    web_sys::console::log_1(&"🧩 init drag-drop listeners".into());

    // A) Helper API on the Webview object (SYNC getter; do NOT await).
    let webview = getCurrentWebview();
    if !webview.is_undefined() && !webview.is_null() {
        if let Ok(on_fn) = js_sys::Reflect::get(&webview, &JsValue::from_str("onDragDropEvent")) {
            if on_fn.is_function() {
                let on = js_sys::Function::from(on_fn);
                let handler_q = q.clone();
                let handler_page = page.clone();

                // Explicit type removes E0283 ambiguity.
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                    web_sys::console::log_1(&"🔥 onDragDropEvent fired".into());
                    log_json("event", &event);
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(JsValue::UNDEFINED);
                    log_json("payload", &payload);

                    let typ = js_sys::Reflect::get(&payload, &JsValue::from_str("type"))
                        .ok().and_then(|v| v.as_string()).unwrap_or_default();
                    web_sys::console::log_1(&JsValue::from_str(&format!("type={}", typ)));

                    if typ == "drop" {
                        if let Ok(paths) = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")) {
                            let arr = js_sys::Array::from(&paths);
                            web_sys::console::log_1(&JsValue::from_str(&format!("paths len={}", arr.length())));
                            if arr.length() > 0 {
                                if let Some(path) = arr.get(0).as_string() {
                                    handle_dropped_path(path, handler_q.clone(), handler_page.clone());
                                }
                            }
                        }
                    }
                });

                // Call onDragDropEvent(handler)
                let _ = on.call1(&webview, handler.as_ref().unchecked_ref());
                handler.forget(); // keep alive
                web_sys::console::log_1(&"✅ onDragDropEvent listener attached".into());
            } else {
                web_sys::console::log_1(&"ℹ️ onDragDropEvent is not a function; using raw event fallback".into());
            }
        }
    }

    // B) Raw event fallback (Tauri v2 DRAG_* events).
    let fallback_q = q.clone();
    let fallback_page = page.clone();
    let raw = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
        web_sys::console::log_1(&"🔥 raw listen('tauri://drag-drop') fired".into());
        log_json("evt", &evt);
        if let Ok(obj) = evt.dyn_into::<js_sys::Object>() {
            if let Ok(payload) = js_sys::Reflect::get(&obj, &JsValue::from_str("payload")) {
                log_json("payload", &payload);
                let typ = js_sys::Reflect::get(&payload, &JsValue::from_str("type"))
                    .ok().and_then(|v| v.as_string()).unwrap_or_default();
                web_sys::console::log_1(&JsValue::from_str(&format!("type={}", typ)));
                if typ == "drop" {
                    if let Ok(paths) = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")) {
                        let arr = js_sys::Array::from(&paths);
                        web_sys::console::log_1(&JsValue::from_str(&format!("paths len={}", arr.length())));
                        if arr.length() > 0 {
                            if let Some(path) = arr.get(0).as_string() {
                                handle_dropped_path(path, fallback_q.clone(), fallback_page.clone());
                            }
                        }
                    }
                }
            }
        }
    });
    let _ = listen("tauri://drag-drop", &raw).await;
    raw.forget();
    web_sys::console::log_1(&"✅ raw tauri://drag-drop listener attached".into());

    // Optional visibility while dragging over the window
    let enter = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        web_sys::console::log_1(&"🟢 tauri://drag-enter".into());
    });
    let _ = listen("tauri://drag-enter", &enter).await;
    enter.forget();

    let over = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        web_sys::console::log_1(&"🟡 tauri://drag-over".into());
    });
    let _ = listen("tauri://drag-over", &over).await;
    over.forget();

    let leave = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        web_sys::console::log_1(&"⚪ tauri://drag-leave".into());
    });
    let _ = listen("tauri://drag-leave", &leave).await;
    leave.forget();
}

#[function_component(App)]
pub fn app() -> Html {
    let _greet_input_ref = use_node_ref();

    let _name = use_state(|| String::new());
    let page = use_state(|| Page::Home);
    let queue_rows = use_state(|| Vec::<ClipRow>::new());

    let on_csv_load = {
        let queue_rows = queue_rows.clone();
        let page = page.clone();
        Callback::from(move |csv_text: String| {
            let rows = parse_csv(&csv_text);
            queue_rows.set(rows);
            page.set(Page::Downloads);
        })
    };

    let on_open_file = {
        let on_csv_load = on_csv_load.clone();
        Callback::from(move |_: ()| {
            let on_csv_load = on_csv_load.clone();
            spawn_local(async move {
                let csv_js = invoke("pick_csv_and_read", JsValue::NULL).await;
                if let Some(csv_text) = csv_js.as_string() {
                    on_csv_load.emit(csv_text);
                }
            });
        })
    };

    // Start listeners at mount
    {
        let q = queue_rows.clone();
        let p = page.clone();
        use_effect_with((), move |_| {
            spawn_local(start_dragdrop_listener(q.clone(), p.clone()));
            || ()
        });
    }

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
        Page::Home => html! { <pages::home::HomePage on_open_file={on_open_file} on_csv_load={on_csv_load.clone()} /> },
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
