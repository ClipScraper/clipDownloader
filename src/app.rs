use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::pages;
use crate::pages::settings::Settings;
use crate::types::{ClipRow, Platform, ContentType};
use yew::prelude::*;
use std::cell::RefCell;
use crate::components::sidebar::Sidebar;
use crate::log;
use std::collections::HashMap;
use crate::pages::downloads::ActiveDownload;

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
pub enum Page {Home, Downloads, Library, Settings, Extension, Sponsor}

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
    log::info("csv_drop_request", serde_json::json!({ "path": path }));
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path })).unwrap();
        match invoke("read_csv_from_path", args).await {
            Ok(_) => {
                log::info("csv_drop_imported", serde_json::json!({ "status": "ok" }));
                web_sys::console::log_1(&"✅ Imported CSV from drop (backend)".into())
            }
            Err(e) => {
                log::error("csv_drop_failed", serde_json::json!({ "error": format!("{e:?}") }));
                log_invoke_err("read_csv_from_path", e)
            }
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
        log::debug("dragdrop_listener_attached", serde_json::json!({ "attached": attached }));
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

pub enum MoveBackItem {
    Platform(Platform),
    Collection(Platform, String, ContentType),
    Row(String),
}

#[function_component(App)]
pub fn app() -> Html {
    let page = use_state(|| Page::Home);
    let settings = use_state(Settings::default);

    let backlog_rows = use_state(|| Vec::<ClipRow>::new());
    let queue_rows   = use_state(|| Vec::<ClipRow>::new());

    let active_downloads = use_state(HashMap::<String, ActiveDownload>::new);
    let is_downloading = use_state(|| false);
    let is_paused = use_state(|| true);

    {
        let settings = settings.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Ok(loaded) = invoke("load_settings", JsValue::NULL).await {
                    if let Ok(s) = serde_wasm_bindgen::from_value(loaded) {
                        settings.set(s);
                    }
                }
            });
            || ()
        });
    }

    {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        let is_paused = is_paused.clone();
        let settings = settings.clone();

        use_effect_with((*page, settings.download_automatically), move |(p, auto)| {
            if *p == Page::Downloads {
                is_paused.set(!*auto);
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

    {
        let active_downloads = active_downloads.clone();
        let is_downloading = is_downloading.clone();
        let queue_rows = queue_rows.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                #[derive(serde::Deserialize)]
                struct DownloadResult { url: String, success: bool, message: String }
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(event.clone());
                    if let Ok(dr) = serde_wasm_bindgen::from_value::<DownloadResult>(payload) {
                        let msg = dr.message.clone();
                        let is_complete =
                            msg.starts_with("Saved") ||
                            msg.starts_with("Failed") ||
                            msg.starts_with("File already exists");
                        if is_complete {
                            log::info("download_complete", serde_json::json!({ "url": dr.url, "success": dr.success, "message": msg }));
                            if !dr.success {
                                if let Some(active) = active_downloads.get(&dr.url) {
                                    let mut q = (*queue_rows).clone();
                                    q.insert(0, active.row.clone());
                                    queue_rows.set(q);
                                }
                            }
                            let mut new_map = (*active_downloads).clone();
                            new_map.remove(&dr.url);
                            active_downloads.set(new_map);
                        } else {
                            let mut new_map = (*active_downloads).clone();
                            if let Some(active) = new_map.get_mut(&dr.url) {
                                active.progress = msg;
                            }
                            active_downloads.set(new_map);
                        }
                    }
                });
                let _ = listen("download-status", &handler).await;
                handler.forget();
            });
            || ()
        });
    }

    let start_next_download = {
        let queue_rows = queue_rows.clone();
        let active_downloads = active_downloads.clone();
        Callback::from(move |_| {
            if let Some(next) = (*queue_rows).get(0).cloned() {
                log::info("queue_autostart", serde_json::json!({ "url": next.link }));
                let mut q = (*queue_rows).clone();
                q.remove(0);
                queue_rows.set(q);

                let mut new_map = (*active_downloads).clone();
                new_map.insert(next.link.clone(), ActiveDownload {
                    row: next.clone(),
                    progress: "Starting...".to_string(),
                });
                active_downloads.set(new_map);

                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(
                        &serde_json::json!({ "url": next.link })
                    ).unwrap();
                    if let Err(e) = invoke("download_url", args).await {
                        log_invoke_err("download_url", e);
                    }
                });
            }
        })
    };

    {
        let page = page.clone();
        let active_downloads = active_downloads.clone();
        let queue_rows = queue_rows.clone();
        let is_paused = is_paused.clone();
        let settings = settings.clone();
        let start_next_download = start_next_download.clone();

        use_effect_with(
            (
                *page,
                settings.download_automatically,
                settings.keep_downloading_on_other_pages,
                (*queue_rows).len(),
                active_downloads.len(),
                *is_paused,
            ),
            move |(p, auto, keep, qlen, active_len, paused)| {
                let on_dl_page = *p == Page::Downloads;
                let can_download = *auto && (*keep || on_dl_page);
                if *qlen > 0 && !*paused && *active_len < settings.parallel_downloads as usize && can_download {
                    start_next_download.emit(());
                }
                || ()
            },
        );
    }

    let on_toggle_pause = {
        let is_paused = is_paused.clone();
        let active_downloads = active_downloads.clone();
        let queue_rows = queue_rows.clone();
        Callback::from(move |_| {
            let going_to_pause = !*is_paused;
            log::info("queue_toggle", serde_json::json!({ "pausing": going_to_pause }));
            if going_to_pause && !active_downloads.is_empty() {
                let mut q = (*queue_rows).clone();
                for (url, active) in active_downloads.iter() {
                    q.insert(0, active.row.clone());
                    let url_clone = url.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "url": url_clone })).unwrap();
                        let _ = invoke("cancel_download", args).await;
                    });
                }
                queue_rows.set(q);
                active_downloads.set(HashMap::new());
            }
            is_paused.set(!*is_paused);
        })
    };

    let on_delete = {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        Callback::from(move |item: DeleteItem| {
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
        Callback::from(move |item: crate::app::MoveItem| {
            match item {
                MoveItem::Platform(plat) => {
                    let plat_str = crate::types::platform_str(&plat).to_string();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "platform": plat_str })).unwrap();
                        let _ = invoke("move_platform_to_queue", args).await;
                    });
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

    let on_move_to_backlog = {
        let backlog_rows = backlog_rows.clone();
        let queue_rows = queue_rows.clone();
        Callback::from(move |item: crate::app::MoveBackItem| {
            match item {
                MoveBackItem::Platform(plat) => {
                    let plat_str = crate::types::platform_str(&plat).to_string();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "platform": plat_str })).unwrap();
                        let _ = invoke("move_platform_to_backlog", args).await;
                    });

                    let mut moved = Vec::new();
                    let mut kept  = Vec::new();
                    for r in (*queue_rows).clone() {
                        if r.platform == plat { moved.push(r); } else { kept.push(r); }
                    }
                    if !moved.is_empty() {
                        let mut b = (*backlog_rows).clone();
                        b.extend(moved);
                        backlog_rows.set(b);
                    }
                    queue_rows.set(kept);
                }
                MoveBackItem::Collection(plat, handle, ctype) => {
                    let p = crate::types::platform_str(&plat).to_string();
                    let t = crate::types::content_type_str(&ctype).to_string();
                    let h = handle.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({"platform": p, "handle": h, "content_type": t})).unwrap();
                        let _ = invoke("move_collection_to_backlog", args).await;
                    });

                    let mut moved = Vec::new();
                    let mut kept  = Vec::new();
                    for r in (*queue_rows).clone() {
                        if r.platform == plat && r.handle == handle && r.content_type == ctype { moved.push(r); } else { kept.push(r); }
                    }
                    if !moved.is_empty() {
                        let mut b = (*backlog_rows).clone();
                        b.extend(moved);
                        backlog_rows.set(b);
                    }
                    queue_rows.set(kept);
                }
                MoveBackItem::Row(link) => {
                    let link_for_backend = link.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "link": link_for_backend })).unwrap();
                        let _ = invoke("move_link_to_backlog", args).await;
                    });

                    let mut moved_one: Option<ClipRow> = None;
                    let kept: Vec<ClipRow> = (*queue_rows).clone().into_iter().filter(|r| {
                        if r.link == link && moved_one.is_none() {
                            moved_one = Some(r.clone());
                            false
                        } else {
                            true
                        }
                    }).collect();

                    if let Some(row) = moved_one {
                        let mut b = (*backlog_rows).clone();
                        b.push(row);
                        backlog_rows.set(b);
                    }
                    queue_rows.set(kept);
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

    let body = match *page {
        Page::Home          => html! { <pages::home::HomePage on_open_file={on_open_file} on_csv_load={on_csv_load.clone()} /> },
        Page::Downloads     => html! {
            <pages::downloads::DownloadsPage
                backlog={(*backlog_rows).clone()}
                queue={(*queue_rows).clone()}
                active={
                    (*active_downloads).values().cloned().collect::<Vec<_>>()
                }
                paused = {*is_paused}
                on_toggle_pause={on_toggle_pause}
                on_delete={on_delete}
                on_move_to_queue={on_move_to_queue}
                on_move_to_backlog={on_move_to_backlog}
            />
        },
        Page::Library       => html! { <pages::library::LibraryPage /> },
        Page::Settings      => html! { <pages::settings::SettingsPage /> },
        Page::Extension     => html! { <pages::extension::ExtensionPage /> },
        Page::Sponsor       => html! { <pages::sponsor::SponsorPage /> },
    };

    html! { <><Sidebar page={page} />{ body }</> }
}
