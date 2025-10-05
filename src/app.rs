use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew_icons::{Icon, IconId};
use crate::pages;
use crate::types::{ClipRow, Platform, ContentType};
use yew::prelude::*;
use std::cell::RefCell;

// ─────────────────────────────────────────────────────────────────────────────
// Tauri v2 JS bridges

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, f: &Closure<dyn FnMut(JsValue)>) -> JsValue;
    #[wasm_bindgen(js_namespace = ["window","__TAURI__","webview"])]
    fn getCurrentWebview() -> JsValue;
}

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
// Drop de-dupe guard
thread_local! {
    static LAST_DROP: RefCell<(String, f64)> = RefCell::new(("".to_string(), 0.0));
}
fn now_ms() -> f64 { js_sys::Date::now() }
fn should_handle_drop(path: &str) -> bool {
    let t = now_ms();
    let mut allow = true;
    LAST_DROP.with(|cell| {
        let mut prev = cell.borrow_mut();
        let same = prev.0 == path;
        let recent = t - prev.1 < 1000.0;
        if same && recent {
            allow = false;
        } else {
            *prev = (path.to_string(), t);
        }
    });
    allow
}

fn spawn_import_from_path(path: String) {
    if !should_handle_drop(&path) {
        web_sys::console::log_1(&format!("⏭️ Ignored duplicate drop for {path}").into());
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

async fn start_dragdrop_listener() {
    web_sys::console::log_1(&"🧩 init drag-drop listener".into());
    let mut attached = false;

    let webview = getCurrentWebview();
    if !webview.is_undefined() && !webview.is_null() {
        if let Ok(on_fn) = js_sys::Reflect::get(&webview, &JsValue::from_str("onDragDropEvent")) {
            if on_fn.is_function() {
                let on = js_sys::Function::from(on_fn);
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                    web_sys::console::log_1(&"🔥 onDragDropEvent fired".into());
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(event.clone());
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

    if !attached {
        let raw = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            web_sys::console::log_1(&"🔥 raw listen('tauri://drag-drop') fired".into());
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
    }
}

/* ---------------- movement to Queue types ---------------- */
pub enum DeleteItem {
    Platform(Platform),
    Collection(Platform, String, ContentType),
    Row(String),
}
pub enum MoveItem {
    Platform(Platform),
    Collection(Platform, String, ContentType),
    Row(String),
}

#[function_component(App)]
pub fn app() -> Html {
    let page = use_state(|| Page::Home);

    let backlog_rows = use_state(|| Vec::<ClipRow>::new());
    let queue_rows   = use_state(|| Vec::<ClipRow>::new());

    // Load both sections when entering Downloads
    {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        let dep = *page;
        use_effect_with(dep, move |p| {
            if *p == Page::Downloads {
                spawn_local(async move {
                    if let Ok(js) = invoke("list_backlog", JsValue::NULL).await {
                        if let Ok(rows) = serde_wasm_bindgen::from_value::<Vec<ClipRow>>(js) {
                            backlog_rows.set(rows);
                        }
                    }
                    if let Ok(js) = invoke("list_queue", JsValue::NULL).await {
                        if let Ok(rows) = serde_wasm_bindgen::from_value::<Vec<ClipRow>>(js) {
                            queue_rows.set(rows);
                        }
                    }
                });
            }
            || ()
        });
    }

    let on_delete = {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        Callback::from(move |item: DeleteItem| {
            // Delete only affects the currently shown lists in-memory.
            let mut trim = |v: Vec<ClipRow>| -> Vec<ClipRow> {
                match &item {
                    DeleteItem::Platform(p) => v.into_iter().filter(|r| r.platform != *p).collect(),
                    DeleteItem::Collection(p, h, t) => v.into_iter().filter(|r| !(r.platform == *p && r.handle == *h && r.content_type == *t)).collect(),
                    DeleteItem::Row(link) => v.into_iter().filter(|r| r.link != *link).collect(),
                }
            };
            backlog_rows.set(trim((*backlog_rows).clone()));
            queue_rows.set(trim((*queue_rows).clone()));
        })
    };

    let on_move_to_queue = {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        Callback::from(move |item: MoveItem| {
            match item {
                MoveItem::Platform(plat) => {
                    let plat_str = crate::types::platform_str(&plat).to_string();
                    // backend mutation
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "platform": plat_str })).unwrap();
                        let _ = invoke("move_platform_to_queue", args).await;
                    });
                    // front-end state move
                    let mut moved = Vec::new();
                    let mut kept  = Vec::new();
                    for r in (*backlog_rows).clone() {
                        if r.platform == plat { moved.push(r); } else { kept.push(r); }
                    }
                    if !moved.is_empty() {
                        let mut q = (*queue_rows).clone();
                        q.extend(moved);
                        queue_rows.set(q);
                    }
                    backlog_rows.set(kept);
                }
                MoveItem::Collection(plat, handle, ctype) => {
                    let p = crate::types::platform_str(&plat).to_string();
                    let t = crate::types::content_type_str(&ctype).to_string();
                    let h = handle.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({"platform": p, "handle": h, "content_type": t})).unwrap();
                        let _ = invoke("move_collection_to_queue", args).await;
                    });

                    let mut moved = Vec::new();
                    let mut kept  = Vec::new();
                    for r in (*backlog_rows).clone() {
                        if r.platform == plat && r.handle == handle && r.content_type == ctype { moved.push(r); } else { kept.push(r); }
                    }
                    if !moved.is_empty() {
                        let mut q = (*queue_rows).clone();
                        q.extend(moved);
                        queue_rows.set(q);
                    }
                    backlog_rows.set(kept);
                }
                MoveItem::Row(link) => {
                    let link_for_backend = link.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "link": link_for_backend })).unwrap();
                        let _ = invoke("move_link_to_queue", args).await;
                    });

                    let mut moved_one: Option<ClipRow> = None;
                    let kept: Vec<ClipRow> = (*backlog_rows).clone().into_iter().filter(|r| {
                        if r.link == link && moved_one.is_none() {
                            moved_one = Some(r.clone());
                            false
                        } else {
                            true
                        }
                    }).collect();

                    if let Some(row) = moved_one {
                        let mut q = (*queue_rows).clone();
                        q.push(row);
                        queue_rows.set(q);
                    }
                    backlog_rows.set(kept);
                }
            }
        })
    };

    let on_csv_load = Callback::from(move |_csv_text: String| {});
    let on_open_file = Callback::from(move |_: ()| {
        spawn_local(async move {
            match invoke("pick_csv_and_read", JsValue::NULL).await {
                Ok(_) => web_sys::console::log_1(&"✅ Imported CSV from picker (backend)".into()),
                Err(e) => log_invoke_err("pick_csv_and_read", e),
            }
        });
    });

    { use_effect_with((), move |_| { spawn_local(start_dragdrop_listener()); || () }); }

    let set_page = |p: Page, page: UseStateHandle<Page>| Callback::from(move |_| page.set(p));

    let sidebar = {
        let page = page.clone();
        html! {
            <aside class="sidebar">
                <button class="nav-btn" onclick={set_page(Page::Home, page.clone())} title="Home"><Icon icon_id={IconId::LucideHome} width={"28"} height={"28"} /></button>
                <button class="nav-btn" onclick={set_page(Page::Downloads, page.clone())} title="Downloads"><Icon icon_id={IconId::LucideDownload} width={"28"} height={"28"} /></button>
                <button class="nav-btn" onclick={set_page(Page::Library, page.clone())} title="Library"><Icon icon_id={IconId::LucideLibrary} width={"28"} height={"28"} /></button>
                <button class="nav-btn" onclick={set_page(Page::Settings, page.clone())} title="Settings"><Icon icon_id={IconId::LucideSettings} width={"28"} height={"28"} /></button>
            </aside>
        }
    };

    let body = match *page {
        Page::Home          => html! { <pages::home::HomePage on_open_file={on_open_file} on_csv_load={on_csv_load.clone()} /> },
        Page::Downloads     => html! { <pages::downloads::DownloadsPage backlog={(*backlog_rows).clone()} queue={(*queue_rows).clone()} on_delete={on_delete} on_move_to_queue={on_move_to_queue} />},
        Page::Library       => html! { <pages::library::LibraryPage /> },
        Page::Settings      => html! { <pages::settings::SettingsPage /> },
    };

    html! { <>{ sidebar }{ body }</> }
}
