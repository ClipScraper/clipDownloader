use crate::components::sidebar::Sidebar;
use crate::log;
use crate::pages;
use crate::pages::downloads::ActiveDownload;
use crate::pages::settings::Settings;
use crate::types::{ClipRow, ContentType, DownloadStatus, Platform};
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, f: &Closure<dyn FnMut(JsValue)>) -> JsValue;
    #[wasm_bindgen(js_namespace = ["window","__TAURI__","webview"])]
    fn getCurrentWebview() -> JsValue;
}

pub fn log_invoke_err(cmd: &str, e: JsValue) {
    web_sys::console::error_2(&format!("invoke({cmd}) failed").into(), &e);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Downloads,
    Library,
    Settings,
    Extension,
    Sponsor,
}

#[derive(Clone, Debug, PartialEq)]
struct DownloadEntry {
    row: ClipRow,
    progress: f32,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

thread_local! {
    static LAST_DROP: RefCell<(String, f64)> = RefCell::new(("".to_string(), 0.0));
}
fn now_ms() -> f64 {
    js_sys::Date::now()
}
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
                log::error(
                    "csv_drop_failed",
                    serde_json::json!({ "error": format!("{e:?}") }),
                );
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
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    if typ == "drop" {
                        if let Ok(paths) =
                            js_sys::Reflect::get(&payload, &JsValue::from_str("paths"))
                        {
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
        log::debug(
            "dragdrop_listener_attached",
            serde_json::json!({ "attached": attached }),
        );
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

    let downloads = use_state(HashMap::<i64, DownloadEntry>::new);
    let paused = use_state(|| false);

    {
        let settings = settings.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Ok(loaded) = invoke("load_settings", JsValue::NULL).await {
                    if let Ok(s) = serde_wasm_bindgen::from_value::<Settings>(loaded) {
                        // Update local state
                        settings.set(s.clone());
                        // Ensure backend DownloadManager pause state matches settings on boot
                        let paused = !s.download_automatically;
                        let args =
                            serde_wasm_bindgen::to_value(&serde_json::json!({ "paused": paused }))
                                .unwrap();
                        let _ = invoke("set_download_paused", args).await;
                        // Also refresh runtime parameters (e.g., parallel downloads)
                        let _ = invoke("refresh_download_settings", JsValue::NULL).await;
                    }
                }
            });
            || ()
        });
    }

    {
        let paused_state = paused.clone();
        use_effect_with(settings.download_automatically, move |auto| {
            paused_state.set(!*auto);
            || ()
        });
    }

    {
        let downloads = downloads.clone();
        use_effect_with(*page, move |p| {
            if *p == Page::Downloads {
                spawn_local(async move {
                    if let Ok(js) = invoke("list_downloads", JsValue::NULL).await {
                        if let Ok(rows) = serde_wasm_bindgen::from_value::<Vec<ClipRow>>(js) {
                            // Debug: log initial list snapshot
                            {
                                let mut cnt_backlog = 0usize;
                                let mut cnt_queue = 0usize;
                                let mut cnt_down = 0usize;
                                let mut cnt_done = 0usize;
                                let mut cnt_err = 0usize;
                                let mut cnt_cancel = 0usize;
                                for r in &rows {
                                    match r.status {
                                        DownloadStatus::Backlog => cnt_backlog += 1,
                                        DownloadStatus::Queued => cnt_queue += 1,
                                        DownloadStatus::Downloading => cnt_down += 1,
                                        DownloadStatus::Done => cnt_done += 1,
                                        DownloadStatus::Error => cnt_err += 1,
                                        DownloadStatus::Canceled => cnt_cancel += 1,
                                    }
                                }
                                web_sys::console::log_1(
                                    &format!(
                                        "[UI] list_downloads loaded: backlog={} queue={} downloading={} done={} error={} canceled={}",
                                        cnt_backlog, cnt_queue, cnt_down, cnt_done, cnt_err, cnt_cancel
                                    )
                                    .into(),
                                );
                            }
                            let mut map = HashMap::new();
                            let mut autostart_ids: Vec<i64> = Vec::new();
                            for row in rows {
                                // Only keep items relevant to the Downloads page
                                // Drop rows that are already finished (or otherwise not actionable here).
                                if matches!(
                                    row.status,
                                    DownloadStatus::Done | DownloadStatus::Error | DownloadStatus::Canceled
                                ) {
                                    continue;
                                }
                                if row.status == DownloadStatus::Queued {
                                    autostart_ids.push(row.id);
                                }
                                map.insert(
                                    row.id,
                                    DownloadEntry {
                                        row,
                                        progress: 0.0,
                                        downloaded_bytes: 0,
                                        total_bytes: None,
                                    },
                                );
                            }
                            downloads.set(map);
                            // Attempt autostart for any queued items (safe - backend de-dupes)
                            if !autostart_ids.is_empty() {
                                web_sys::console::log_1(
                                    &format!(
                                        "[UI] queue_autostart: enqueuing {} ids",
                                        autostart_ids.len()
                                    )
                                    .into(),
                                );
                                let args = serde_wasm_bindgen::to_value(
                                    &serde_json::json!({ "ids": autostart_ids }),
                                )
                                .unwrap();
                                if let Err(e) = invoke("enqueue_downloads", args).await {
                                    log_invoke_err("enqueue_downloads(queue_autostart)", e);
                                }
                            } else {
                                web_sys::console::log_1(
                                    &"[UI] queue_autostart: nothing to enqueue".into(),
                                );
                            }
                        }
                    }
                });
            }
            || ()
        });
    }

    {
        let downloads = downloads.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                #[derive(serde::Deserialize, Debug)]
                #[serde(tag = "type")]
                enum DownloadEventPayload {
                    StatusChanged {
                        id: i64,
                        status: DownloadStatus,
                    },
                    Progress {
                        id: i64,
                        progress: f32,
                        downloaded_bytes: u64,
                        total_bytes: Option<u64>,
                    },
                    Message {
                        id: i64,
                        message: String,
                    },
                }
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(event.clone());
                    if let Ok(evt) = serde_wasm_bindgen::from_value::<DownloadEventPayload>(payload)
                    {
                        // Debug: echo raw event to console
                        web_sys::console::log_1(&format!("[UI] download_event {:?}", evt).into());
                        let mut map = (*downloads).clone();
                        match evt {
                            DownloadEventPayload::StatusChanged { id, status } => {
                                // Terminal state: drop from local map and hard-refresh from DB for correctness
                                if matches!(
                                    status,
                                    DownloadStatus::Done
                                        | DownloadStatus::Error
                                        | DownloadStatus::Canceled
                                ) {
                                    let had = map.remove(&id).is_some();
                                    web_sys::console::log_1(
                                        &format!(
                                            "[UI] StatusChanged => terminal; removed id={} present_before={} remaining={}",
                                            id,
                                            had,
                                            map.len()
                                        )
                                        .into(),
                                    );
                                    downloads.set(map);
                                    // Refresh from DB to ensure UI is fully in sync
                                    let downloads_ref = downloads.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if let Ok(js) = invoke("list_downloads", JsValue::NULL).await {
                                            if let Ok(rows) =
                                                serde_wasm_bindgen::from_value::<Vec<ClipRow>>(js)
                                            {
                                                use std::collections::HashMap;
                                                let mut fresh: HashMap<i64, DownloadEntry> =
                                                    HashMap::new();
                                                let mut cnt = 0usize;
                                                for row in rows {
                                                    if matches!(
                                                        row.status,
                                                        DownloadStatus::Done
                                                            | DownloadStatus::Error
                                                            | DownloadStatus::Canceled
                                                    ) {
                                                        continue;
                                                    }
                                                    cnt += 1;
                                                    fresh.insert(
                                                        row.id,
                                                        DownloadEntry {
                                                            row,
                                                            progress: 0.0,
                                                            downloaded_bytes: 0,
                                                            total_bytes: None,
                                                        },
                                                    );
                                                }
                                                web_sys::console::log_1(
                                                    &format!(
                                                        "[UI] post-refresh list_downloads active_count={}",
                                                        cnt
                                                    )
                                                    .into(),
                                                );
                                                downloads_ref.set(fresh);
                                            }
                                        }
                                    });
                                } else {
                                    if let Some(entry) = map.get_mut(&id) {
                                        entry.row.status = status;
                                        web_sys::console::log_1(
                                            &format!(
                                                "[UI] StatusChanged => non-terminal; id={} status={:?}",
                                                id, status
                                            )
                                            .into(),
                                        );
                                        downloads.set(map);
                                        // Also refresh to pick up any other rows that may have transitioned
                                        let downloads_ref = downloads.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if let Ok(js) =
                                                invoke("list_downloads", JsValue::NULL).await
                                            {
                                                if let Ok(rows) =
                                                    serde_wasm_bindgen::from_value::<Vec<ClipRow>>(
                                                        js,
                                                    )
                                                {
                                                    use std::collections::HashMap;
                                                    let mut fresh: HashMap<i64, DownloadEntry> =
                                                        HashMap::new();
                                                    for row in rows {
                                                        if matches!(
                                                            row.status,
                                                            DownloadStatus::Done
                                                                | DownloadStatus::Error
                                                                | DownloadStatus::Canceled
                                                        ) {
                                                            continue;
                                                        }
                                                        fresh.insert(
                                                            row.id,
                                                            DownloadEntry {
                                                                row,
                                                                progress: 0.0,
                                                                downloaded_bytes: 0,
                                                                total_bytes: None,
                                                            },
                                                        );
                                                    }
                                                    downloads_ref.set(fresh);
                                                }
                                            }
                                        });
                                    } else {
                                        log::info(
                                            "download_event_unknown",
                                            serde_json::json!({ "id": id, "status": format!("{:?}", status) }),
                                        );
                                        web_sys::console::log_1(
                                            &format!(
                                                "[UI] StatusChanged for unknown id={}, ignoring",
                                                id
                                            )
                                            .into(),
                                        );
                                        // Fallback: fetch fresh snapshot so we can include this id
                                        let downloads_ref = downloads.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if let Ok(js) =
                                                invoke("list_downloads", JsValue::NULL).await
                                            {
                                                if let Ok(rows) =
                                                    serde_wasm_bindgen::from_value::<Vec<ClipRow>>(
                                                        js,
                                                    )
                                                {
                                                    use std::collections::HashMap;
                                                    let mut fresh: HashMap<i64, DownloadEntry> =
                                                        HashMap::new();
                                                    for row in rows {
                                                        if matches!(
                                                            row.status,
                                                            DownloadStatus::Done
                                                                | DownloadStatus::Error
                                                                | DownloadStatus::Canceled
                                                        ) {
                                                            continue;
                                                        }
                                                        fresh.insert(
                                                            row.id,
                                                            DownloadEntry {
                                                                row,
                                                                progress: 0.0,
                                                                downloaded_bytes: 0,
                                                                total_bytes: None,
                                                            },
                                                        );
                                                    }
                                                    web_sys::console::log_1(
                                                        &"[UI] refreshed map after unknown id"
                                                            .into(),
                                                    );
                                                    downloads_ref.set(fresh);
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            DownloadEventPayload::Progress {
                                id,
                                progress,
                                downloaded_bytes,
                                total_bytes,
                            } => {
                                if let Some(entry) = map.get_mut(&id) {
                                    entry.progress = progress;
                                    entry.downloaded_bytes = downloaded_bytes;
                                    entry.total_bytes = total_bytes;
                                    web_sys::console::log_1(
                                        &format!(
                                            "[UI] Progress id={} {:.0}% bytes={}/{:?}",
                                            id,
                                            progress * 100.0,
                                            downloaded_bytes,
                                            total_bytes
                                        )
                                        .into(),
                                    );
                                    downloads.set(map);
                                }
                            }
                            DownloadEventPayload::Message { id, message } => {
                                log::info(
                                    "download_event_message",
                                    serde_json::json!({ "id": id, "message": message.clone() }),
                                );
                                web_sys::console::log_1(
                                    &format!("[UI] Message id={} {}", id, message).into(),
                                );
                            }
                        }
                    }
                });
                let _ = listen("download_event", &handler).await;
                handler.forget();
            });
            || ()
        });
    }

    let on_toggle_pause = {
        let paused_state = paused.clone();
        Callback::from(move |_| {
            let next = !*paused_state;
            paused_state.set(next);
            log::info("queue_toggle", serde_json::json!({ "paused": next }));
            spawn_local(async move {
                let args =
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "paused": next })).unwrap();
                if let Err(e) = invoke("set_download_paused", args).await {
                    log_invoke_err("set_download_paused", e);
                }
            });
        })
    };

    let on_delete = {
        let downloads = downloads.clone();
        Callback::from(move |item: DeleteItem| {
            downloads.set({
                let mut map = (*downloads).clone();
                map.retain(|_, entry| !matches_delete_item(&entry.row, &item));
                map
            });
        })
    };

    let on_move_to_queue = {
        let downloads = downloads.clone();
        Callback::from(move |item: crate::app::MoveItem| {
            let ids: Vec<i64> = (*downloads)
                .values()
                .filter(|entry| {
                    entry.row.status == DownloadStatus::Backlog
                        && matches_move_item(&entry.row, &item)
                })
                .map(|entry| entry.row.id)
                .collect();
            if ids.is_empty() {
                return;
            }
            spawn_local(async move {
                let args =
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "ids": ids })).unwrap();
                if let Err(e) = invoke("enqueue_downloads", args).await {
                    log_invoke_err("enqueue_downloads", e);
                }
            });
        })
    };

    let on_move_to_backlog = {
        let downloads = downloads.clone();
        Callback::from(move |item: crate::app::MoveBackItem| {
            let ids: Vec<i64> = (*downloads)
                .values()
                .filter(|entry| {
                    matches_move_back_item(&entry.row, &item)
                        && matches!(
                            entry.row.status,
                            DownloadStatus::Queued | DownloadStatus::Downloading
                        )
                })
                .map(|entry| entry.row.id)
                .collect();
            if ids.is_empty() {
                return;
            }
            spawn_local(async move {
                let args =
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "ids": ids })).unwrap();
                if let Err(e) = invoke("move_downloads_to_backlog", args).await {
                    log_invoke_err("move_downloads_to_backlog", e);
                }
            });
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

    {
        use_effect_with((), move |_| {
            spawn_local(start_dragdrop_listener());
            || ()
        });
    }

    let backlog_rows_vec: Vec<ClipRow> = (*downloads)
        .values()
        .filter(|entry| entry.row.status == DownloadStatus::Backlog)
        .map(|entry| entry.row.clone())
        .collect();
    let queue_rows_vec: Vec<ClipRow> = (*downloads)
        .values()
        .filter(|entry| entry.row.status == DownloadStatus::Queued)
        .map(|entry| entry.row.clone())
        .collect();
    let active_downloads_vec: Vec<ActiveDownload> = (*downloads)
        .values()
        .filter(|entry| entry.row.status == DownloadStatus::Downloading)
        .map(|entry| ActiveDownload {
            row: entry.row.clone(),
            progress: format!("{:.0}%", entry.progress * 100.0),
        })
        .collect();

    let body = match *page {
        Page::Home => {
            html! { <pages::home::HomePage on_open_file={on_open_file} on_csv_load={on_csv_load.clone()} /> }
        }
        Page::Downloads => html! {
            <pages::downloads::DownloadsPage
                backlog={backlog_rows_vec}
                queue={queue_rows_vec}
                active={active_downloads_vec}
                paused = {*paused}
                on_toggle_pause={on_toggle_pause}
                on_delete={on_delete}
                on_move_to_queue={on_move_to_queue}
                on_move_to_backlog={on_move_to_backlog}
            />
        },
        Page::Library => html! { <pages::library::LibraryPage /> },
        Page::Settings => html! { <pages::settings::SettingsPage /> },
        Page::Extension => html! { <pages::extension::ExtensionPage /> },
        Page::Sponsor => html! { <pages::sponsor::SponsorPage /> },
    };

    html! { <><Sidebar page={page} />{ body }</> }
}

fn matches_delete_item(row: &ClipRow, item: &DeleteItem) -> bool {
    match item {
        DeleteItem::Platform(p) => row.platform == *p,
        DeleteItem::Collection(p, handle, ctype) => {
            row.platform == *p && row.handle == *handle && row.content_type == *ctype
        }
        DeleteItem::Row(link) => row.link == *link,
    }
}

fn matches_move_item(row: &ClipRow, item: &MoveItem) -> bool {
    match item {
        MoveItem::Platform(p) => row.platform == *p,
        MoveItem::Collection(p, handle, ctype) => {
            row.platform == *p && row.handle == *handle && row.content_type == *ctype
        }
        MoveItem::Row(link) => row.link == *link,
    }
}

fn matches_move_back_item(row: &ClipRow, item: &MoveBackItem) -> bool {
    match item {
        MoveBackItem::Platform(p) => row.platform == *p,
        MoveBackItem::Collection(p, handle, ctype) => {
            row.platform == *p && row.handle == *handle && row.content_type == *ctype
        }
        MoveBackItem::Row(link) => row.link == *link,
    }
}
