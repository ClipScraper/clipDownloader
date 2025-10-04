// ===== src/app.rs (single drag-drop listener + de-dupe, frontend no-ops after DB import) =====
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
use crate::pages; // declared in main.rs
use crate::types::{ClipRow, Platform, ContentType};
use yew::prelude::*;
use std::cell::RefCell;

// ─────────────────────────────────────────────────────────────────────────────
// Tauri v2 JS bridges

// add `catch` so rejected Promises (backend Err(...)) don't panic WASM.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, f: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window","__TAURI__","webview"])]
    fn getCurrentWebview() -> JsValue;
}

// Small helper to log an invoke error
fn log_invoke_err(cmd: &str, e: JsValue) {
    web_sys::console::error_2(&format!("invoke({cmd}) failed").into(), &e);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {Home, Downloads, Library, Settings}

fn log_json(label: &str, v: &JsValue) {
    let s = js_sys::JSON::stringify(v)
        .ok()
        .and_then(|j| j.as_string())
        .unwrap_or_else(|| "<unstringifiable>".to_string());
    web_sys::console::log_2(&JsValue::from_str(label), &JsValue::from_str(&s));
}

// ─────────────────────────────────────────────────────────────────────────────
// Drop de-dupe guard: ignore the same path twice within 1s
thread_local! {
    static LAST_DROP: RefCell<(String, f64)> = RefCell::new(("".to_string(), 0.0)); // (path, timestamp_ms)
}

// Use Date.now() to avoid needing the web-sys Performance feature.
fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn should_handle_drop(path: &str) -> bool {
    let t = now_ms();
    let mut allow = true;
    LAST_DROP.with(|cell| {
        let mut prev = cell.borrow_mut();
        let same = prev.0 == path;
        let recent = t - prev.1 < 1000.0; // 1 second window
        if same && recent {
            allow = false;
        } else {
            *prev = (path.to_string(), t);
        }
    });
    allow
}

// Spawn the backend import for a given filesystem path.
// We ignore the return (frontend no-ops after DB save).
fn spawn_import_from_path(path: String) {
    if !should_handle_drop(&path) {
        web_sys::console::log_1(&format!("⏭️ Ignored duplicate drop for {}", path).into());
        return;
    }
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path })).unwrap();
        match invoke("read_csv_from_path", args).await {
            Ok(_) => web_sys::console::log_1(&"✅ Imported CSV from drop (backend)".into()),
            Err(e) => log_invoke_err("read_csv_from_path", e),
        }
    });
}

// Start ONE drag-drop listener: prefer onDragDropEvent; otherwise use raw tauri events.
async fn start_dragdrop_listener() {
    web_sys::console::log_1(&"🧩 init drag-drop listener".into());

    let mut attached = false;

    // Try Webview helper API first
    let webview = getCurrentWebview();
    if !webview.is_undefined() && !webview.is_null() {
        if let Ok(on_fn) = js_sys::Reflect::get(&webview, &JsValue::from_str("onDragDropEvent")) {
            if on_fn.is_function() {
                let on = js_sys::Function::from(on_fn);
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                    web_sys::console::log_1(&"🔥 onDragDropEvent fired".into());
                    log_json("event", &event);
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(JsValue::UNDEFINED);
                    log_json("payload", &payload);

                    let typ = js_sys::Reflect::get(&payload, &JsValue::from_str("type"))
                        .ok().and_then(|v| v.as_string()).unwrap_or_default();
                    if typ == "drop" {
                        if let Ok(paths) = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")) {
                            let arr = js_sys::Array::from(&paths);
                            if arr.length() > 0 {
                                if let Some(path) = arr.get(0).as_string() {
                                    spawn_import_from_path(path);
                                }
                            }
                        }
                    }
                });

                let _ = on.call1(&webview, handler.as_ref().unchecked_ref());
                handler.forget();
                attached = true;
                web_sys::console::log_1(&"✅ attached onDragDropEvent listener".into());
            }
        }
    }

    // If helper API not available, attach the raw fallback
    if !attached {
        let raw = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            web_sys::console::log_1(&"🔥 raw listen('tauri://drag-drop') fired".into());
            log_json("evt", &evt);
            if let Ok(obj) = evt.dyn_into::<js_sys::Object>() {
                if let Ok(payload) = js_sys::Reflect::get(&obj, &JsValue::from_str("payload")) {
                    if let Ok(paths) = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")) {
                        let arr = js_sys::Array::from(&paths);
                        if arr.length() > 0 {
                            if let Some(path) = arr.get(0).as_string() {
                                spawn_import_from_path(path);
                            }
                        }
                    }
                }
            }
        });
        let _ = listen("tauri://drag-drop", &raw).await;
        raw.forget();
        web_sys::console::log_1(&"✅ attached raw tauri://drag-drop listener".into());

        // optional: log the other raw drag states
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
}

pub enum DeleteItem {
    Platform(Platform),
    Collection(Platform, String, ContentType),
    Row(String),
}

#[function_component(App)]
pub fn app() -> Html {
    println!("[FRONTEND] [app.rs] [app component]");
    let page = use_state(|| Page::Home);
    let queue_rows = use_state(|| Vec::<ClipRow>::new()); // UI no-ops after DB import.

    let on_delete = {
        let queue_rows = queue_rows.clone();
        Callback::from(move |item: DeleteItem| {
            let current_rows = (*queue_rows).clone();
            let new_rows = match item {
                DeleteItem::Platform(plat) => current_rows.into_iter().filter(|r| r.platform != plat).collect(),
                DeleteItem::Collection(plat, handle, ctype) => current_rows.into_iter().filter(|r| r.platform != plat || r.handle != handle || r.content_type != ctype).collect(),
                DeleteItem::Row(link) => current_rows.into_iter().filter(|r| r.link != link).collect(),
            };
            queue_rows.set(new_rows);
        })
    };

    // No-op: backend imports; nothing else in UI afterwards.
    let on_csv_load = {
        Callback::from(move |_csv_text: String| {
            // intentionally empty
        })
    };

    // "Import list" -> open picker in backend, which imports; we only log success/failure.
    let on_open_file = {
        Callback::from(move |_: ()| {
            spawn_local(async move {
                match invoke("pick_csv_and_read", JsValue::NULL).await {
                    Ok(_) => web_sys::console::log_1(&"✅ Imported CSV from picker (backend)".into()),
                    Err(e) => log_invoke_err("pick_csv_and_read", e),
                }
            });
        })
    };

    {
        use_effect_with((), move |_| {
            spawn_local(start_dragdrop_listener());
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
        Page::Downloads => html! { <pages::downloads::DownloadsPage rows={(*queue_rows).clone()} on_delete={on_delete} /> },
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
