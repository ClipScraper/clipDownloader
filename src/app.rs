use crate::components::sidebar::Sidebar;
use crate::log;
use crate::pages;
use crate::pages::downloads::ActiveDownload;
use crate::pages::settings::Settings;
use crate::types::{ClipRow, ContentType, DownloadStatus, Platform};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
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
    stage_text: String,
    last_message: Option<String>,
}

fn log_download_snapshot(rows: &[ClipRow]) {
    let mut cnt_backlog = 0usize;
    let mut cnt_queue = 0usize;
    let mut cnt_down = 0usize;
    let mut cnt_done = 0usize;
    let mut cnt_err = 0usize;
    let mut cnt_cancel = 0usize;
    for row in rows {
        match row.status {
            DownloadStatus::Backlog => cnt_backlog += 1,
            DownloadStatus::Queued => cnt_queue += 1,
            DownloadStatus::Downloading => cnt_down += 1,
            DownloadStatus::Done => cnt_done += 1,
            DownloadStatus::Error => cnt_err += 1,
            DownloadStatus::Canceled => cnt_cancel += 1,
        }
    }
    web_sys::console::log_1(&format!("[UI] list_downloads loaded: backlog={} queue={} downloading={} done={} error={} canceled={}",cnt_backlog, cnt_queue, cnt_down, cnt_done, cnt_err, cnt_cancel).into());
}

fn default_stage_text(row: &ClipRow) -> String {
    match row.status {
        DownloadStatus::Backlog => "Backlog".into(),
        DownloadStatus::Queued => "Queued".into(),
        DownloadStatus::Downloading => "Preparing download".into(),
        DownloadStatus::Done => "Done".into(),
        DownloadStatus::Error => row.last_error.clone().unwrap_or_else(|| "Failed".into()),
        DownloadStatus::Canceled => "Canceled".into(),
    }
}

fn merge_download_entries(
    previous: &HashMap<i64, DownloadEntry>,
    rows: Vec<ClipRow>,
) -> HashMap<i64, DownloadEntry> {
    let mut map = HashMap::new();

    for row in rows {
        if matches!(row.status, DownloadStatus::Done | DownloadStatus::Canceled) {
            continue;
        }

        let status = row.status;
        let persisted_error = row.last_error.clone();
        let mut entry = previous.get(&row.id).cloned().unwrap_or(DownloadEntry {
            row: row.clone(),
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            stage_text: default_stage_text(&row),
            last_message: persisted_error.clone(),
        });

        entry.row = row;
        if let Some(err) = persisted_error.clone() {
            entry.last_message = Some(err);
        }

        match status {
            DownloadStatus::Backlog | DownloadStatus::Queued => {
                entry.progress = 0.0;
                entry.downloaded_bytes = 0;
                entry.total_bytes = None;
                entry.row.last_error = None;
                entry.last_message = None;
                entry.stage_text = default_stage_text(&entry.row);
            }
            DownloadStatus::Downloading => {
                entry.row.last_error = None;
                if entry.stage_text.is_empty() || entry.stage_text == "Queued" {
                    entry.stage_text = "Preparing download".into();
                }
            }
            DownloadStatus::Error => {
                entry.progress = 0.0;
                entry.downloaded_bytes = 0;
                entry.total_bytes = None;
                if entry.row.last_error.is_none() {
                    entry.row.last_error = entry.last_message.clone();
                }
                entry.stage_text = entry
                    .row
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "Failed".into());
            }
            DownloadStatus::Done | DownloadStatus::Canceled => {}
        }

        map.insert(entry.row.id, entry);
    }

    map
}

fn summarize_download_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "Working...".into();
    }

    if let Some(rest) = trimmed.strip_prefix("Trying ") {
        if let Some((browser, _)) = rest.split_once(" cookies") {
            return format!("Loading {browser} cookies");
        }
    }

    if trimmed.starts_with("Saved ") {
        return "Finalizing download".into();
    }

    if trimmed.starts_with("Prepared output file ") {
        return "Prepared output path".into();
    }

    trimmed.to_string()
}

fn commit_download_map(
    downloads: &UseStateHandle<HashMap<i64, DownloadEntry>>,
    downloads_ref: &Rc<RefCell<HashMap<i64, DownloadEntry>>>,
    next: HashMap<i64, DownloadEntry>,
) {
    *downloads_ref.borrow_mut() = next.clone();
    downloads.set(next);
}

fn spawn_refresh_downloads(
    downloads: UseStateHandle<HashMap<i64, DownloadEntry>>,
    downloads_ref: Rc<RefCell<HashMap<i64, DownloadEntry>>>,
    ready: UseStateHandle<bool>,
) {
    spawn_local(async move {
        match invoke("refresh_downloads_snapshot", JsValue::NULL).await {
            Ok(js) => match serde_wasm_bindgen::from_value::<Vec<ClipRow>>(js) {
                Ok(rows) => {
                    log_download_snapshot(&rows);
                    let previous = downloads_ref.borrow().clone();
                    let next = merge_download_entries(&previous, rows);
                    commit_download_map(&downloads, &downloads_ref, next);
                }
                Err(err) => {
                    web_sys::console::error_1(
                        &format!("deserialize(refresh_downloads_snapshot) failed: {err}").into(),
                    );
                }
            },
            Err(e) => log_invoke_err("refresh_downloads_snapshot", e),
        }
        ready.set(true);
    });
}

fn schedule_download_refresh(
    refresh_pending: Rc<Cell<bool>>,
    downloads: UseStateHandle<HashMap<i64, DownloadEntry>>,
    downloads_ref: Rc<RefCell<HashMap<i64, DownloadEntry>>>,
    ready: UseStateHandle<bool>,
) {
    if refresh_pending.get() {
        return;
    }

    refresh_pending.set(true);
    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(250).await;
        spawn_refresh_downloads(downloads.clone(), downloads_ref.clone(), ready.clone());
        refresh_pending.set(false);
    });
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
    let downloads_ref = use_mut_ref(HashMap::<i64, DownloadEntry>::new);
    let downloads_ready = use_state(|| false);
    let paused = use_state(|| false);

    {
        let settings = settings.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Ok(loaded) = invoke("load_settings", JsValue::NULL).await {
                    if let Ok(s) = serde_wasm_bindgen::from_value::<Settings>(loaded) {
                        settings.set(s.clone());
                        let paused = !s.download_automatically;
                        let args =
                            serde_wasm_bindgen::to_value(&serde_json::json!({ "paused": paused }))
                                .unwrap();
                        let _ = invoke("set_download_paused", args).await;
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
        let downloads_ref = downloads_ref.clone();
        let downloads_ready = downloads_ready.clone();
        use_effect_with((), move |_| {
            spawn_refresh_downloads(
                downloads.clone(),
                downloads_ref.clone(),
                downloads_ready.clone(),
            );
            || ()
        });
    }

    {
        let downloads = downloads.clone();
        let downloads_ref = downloads_ref.clone();
        let downloads_ready = downloads_ready.clone();
        use_effect_with(*page, move |p| {
            if *p == Page::Downloads {
                spawn_refresh_downloads(
                    downloads.clone(),
                    downloads_ref.clone(),
                    downloads_ready.clone(),
                );
            }
            || ()
        });
    }

    {
        let downloads = downloads.clone();
        let downloads_ref = downloads_ref.clone();
        let downloads_ready = downloads_ready.clone();
        use_effect_with((), move |_| {
            let refresh_pending = Rc::new(Cell::new(false));

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
                        let mut map = downloads_ref.borrow().clone();
                        let mut commit = false;
                        let mut should_refresh = false;

                        match evt {
                            DownloadEventPayload::StatusChanged { id, status } => match status {
                                DownloadStatus::Done | DownloadStatus::Canceled => {
                                    if map.remove(&id).is_none() {
                                        should_refresh = true;
                                    } else {
                                        commit = true;
                                    }
                                    should_refresh = true;
                                }
                                DownloadStatus::Error => {
                                    if let Some(entry) = map.get_mut(&id) {
                                        entry.row.status = DownloadStatus::Error;
                                        if entry.row.last_error.is_none() {
                                            entry.row.last_error = entry.last_message.clone();
                                        }
                                        entry.progress = 0.0;
                                        entry.downloaded_bytes = 0;
                                        entry.total_bytes = None;
                                        entry.stage_text = entry
                                            .row
                                            .last_error
                                            .clone()
                                            .unwrap_or_else(|| "Failed".into());
                                        commit = true;
                                    } else {
                                        should_refresh = true;
                                    }
                                    should_refresh = true;
                                }
                                DownloadStatus::Backlog | DownloadStatus::Queued => {
                                    if let Some(entry) = map.get_mut(&id) {
                                        entry.row.status = status;
                                        entry.row.last_error = None;
                                        entry.progress = 0.0;
                                        entry.downloaded_bytes = 0;
                                        entry.total_bytes = None;
                                        entry.stage_text = default_stage_text(&entry.row);
                                        entry.last_message = None;
                                        commit = true;
                                    } else {
                                        should_refresh = true;
                                    }
                                }
                                DownloadStatus::Downloading => {
                                    if let Some(entry) = map.get_mut(&id) {
                                        entry.row.status = DownloadStatus::Downloading;
                                        entry.row.last_error = None;
                                        entry.stage_text = "Preparing download".into();
                                        entry.last_message = None;
                                        commit = true;
                                    } else {
                                        should_refresh = true;
                                    }
                                }
                            },
                            DownloadEventPayload::Progress {
                                id,
                                progress,
                                downloaded_bytes,
                                total_bytes,
                            } => {
                                if let Some(entry) = map.get_mut(&id) {
                                    entry.row.status = DownloadStatus::Downloading;
                                    entry.progress = progress;
                                    entry.downloaded_bytes = downloaded_bytes;
                                    entry.total_bytes = total_bytes;
                                    if progress > 0.0 {
                                        entry.stage_text = "Downloading".into();
                                    }
                                    commit = true;
                                }
                            }
                            DownloadEventPayload::Message { id, message } => {
                                log::info(
                                    "download_event_message",
                                    serde_json::json!({ "id": id, "message": message.clone() }),
                                );
                                if let Some(entry) = map.get_mut(&id) {
                                    entry.last_message = Some(message.clone());
                                    if entry.row.status == DownloadStatus::Error {
                                        entry.row.last_error = Some(message.clone());
                                        entry.stage_text = message;
                                    } else {
                                        entry.stage_text = summarize_download_message(&message);
                                    }
                                    commit = true;
                                }
                            }
                        }

                        if commit {
                            commit_download_map(&downloads, &downloads_ref, map);
                        }
                        if should_refresh {
                            schedule_download_refresh(
                                refresh_pending.clone(),
                                downloads.clone(),
                                downloads_ref.clone(),
                                downloads_ready.clone(),
                            );
                        }
                    }
                });

                let _ = listen("download_event", &handler).await;
                handler.forget();
            });
            || ()
        });
    }

    {
        let downloads = downloads.clone();
        let downloads_ref = downloads_ref.clone();
        let downloads_ready = downloads_ready.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let handler = Closure::<dyn FnMut(JsValue)>::new(move |_event: JsValue| {
                    web_sys::console::log_1(
                        &"[UI] import_completed event received, reloading downloads".into(),
                    );
                    spawn_refresh_downloads(
                        downloads.clone(),
                        downloads_ref.clone(),
                        downloads_ready.clone(),
                    );
                });
                let _ = listen("import_completed", &handler).await;
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
        let downloads_ref = downloads_ref.clone();
        Callback::from(move |item: DeleteItem| {
            let mut map = downloads_ref.borrow().clone();
            map.retain(|_, entry| !matches_delete_item(&entry.row, &item));
            commit_download_map(&downloads, &downloads_ref, map);
        })
    };

    let on_move_to_queue = {
        let downloads_ref = downloads_ref.clone();
        Callback::from(move |item: crate::app::MoveItem| {
            let ids: Vec<i64> = downloads_ref
                .borrow()
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
        let downloads_ref = downloads_ref.clone();
        Callback::from(move |item: crate::app::MoveBackItem| {
            let ids: Vec<i64> = downloads_ref
                .borrow()
                .values()
                .filter(|entry| {
                    matches_move_back_item(&entry.row, &item)
                        && matches!(
                            entry.row.status,
                            DownloadStatus::Queued
                                | DownloadStatus::Downloading
                                | DownloadStatus::Error
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

    let on_retry_issue = Callback::from(move |id: i64| {
        spawn_local(async move {
            let args =
                serde_wasm_bindgen::to_value(&serde_json::json!({ "ids": vec![id] })).unwrap();
            if let Err(e) = invoke("enqueue_downloads", args).await {
                log_invoke_err("enqueue_downloads", e);
            }
        });
    });

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
    let issue_rows_vec: Vec<ClipRow> = (*downloads)
        .values()
        .filter(|entry| entry.row.status == DownloadStatus::Error)
        .map(|entry| {
            let mut row = entry.row.clone();
            if row.last_error.is_none() {
                row.last_error = entry.last_message.clone();
            }
            row
        })
        .collect();
    let active_downloads_vec: Vec<ActiveDownload> = (*downloads)
        .values()
        .filter(|entry| entry.row.status == DownloadStatus::Downloading)
        .map(|entry| ActiveDownload {
            row: entry.row.clone(),
            progress: if entry.progress > 0.0 {
                Some(format!("{:.0}%", entry.progress * 100.0))
            } else {
                None
            },
            stage: entry.stage_text.clone(),
        })
        .collect();

    let body = match *page {
        Page::Home => {
            html! { <pages::home::HomePage on_open_file={on_open_file} on_csv_load={on_csv_load.clone()} /> }
        }
        Page::Downloads => {
            html! {
                <pages::downloads::DownloadsPage
                    backlog={backlog_rows_vec}
                    queue={queue_rows_vec}
                    issues={issue_rows_vec}
                    active={active_downloads_vec}
                    loading={!*downloads_ready}
                    paused = {*paused}
                    on_toggle_pause={on_toggle_pause}
                    on_delete={on_delete}
                    on_move_to_queue={on_move_to_queue}
                    on_move_to_backlog={on_move_to_backlog}
                    on_retry_issue={on_retry_issue}
                />
            }
        }
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
