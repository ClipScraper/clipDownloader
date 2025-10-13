use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::DragEvent;
use yew::prelude::*;
use serde::{Serialize, Deserialize};
use yew_hooks::prelude::*;
use yew_icons::{Icon, IconId};
use crate::log;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DownloadResult {
    success: bool,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> { name: &'a str }

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, f: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub on_open_file: Callback<()>,
    pub on_csv_load: Callback<String>,
}

#[function_component(HomePage)]
pub fn home_page(props: &Props) -> Html {
    println!("[FRONTEND] [pages/home.rs] [home_page component]");
    let greet_input_ref = use_node_ref();
    let name = use_state(|| String::new());
    let download_results = use_state(|| Vec::<DownloadResult>::new());
    let is_downloading = use_state(|| false);
    let download_progress = use_state(|| String::from("Starting download..."));
    let is_valid_url = name.contains("instagram.com")
        || name.contains("tiktok.com")
        || name.contains("youtube.com")
        || name.contains("youtu.be");

    {
        let download_results = download_results.clone();
        let is_downloading = is_downloading.clone();
        let download_progress = download_progress.clone();
        use_effect_once(move || {
            let download_results = download_results.clone();
            let is_downloading_clone = is_downloading.clone();
            let download_progress_clone = download_progress.clone();
            spawn_local(async move {
                let closure = Closure::wrap(Box::new(move |event: JsValue| {
                    let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                        .unwrap_or(event.clone());
                    
                    if let Ok(result) = serde_wasm_bindgen::from_value::<DownloadResult>(payload) {
                        let msg = result.message.clone();
                        let is_complete = msg.starts_with("Saved") || msg.starts_with("Failed") || msg.starts_with("File already exists");
                        if is_complete {
                            log::info("home_download_complete", serde_json::json!({ "success": result.success, "message": msg }));
                            is_downloading_clone.set(false);
                            let mut results = (*download_results).clone();
                            results.push(result);
                            download_results.set(results);
                        } else {
                            // Update progress for non-completion messages
                            download_progress_clone.set(msg.clone());
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                let _ = listen("download-status", &closure).await;
                closure.forget();
            });
            || {}
        });
    }

    let on_input = {
        let name = name.clone();
        Callback::from(move |e: web_sys::InputEvent| {
            if let Some(t) = e.target() {
                if let Ok(inp) = t.dyn_into::<web_sys::HtmlInputElement>() {
                    let value = inp.value();
                    web_sys::console::log_1(&format!("Input changed: {}", value).into());
                    name.set(value);
                }
            }
        })
    };
    let greet = {
        let greet_input_ref = greet_input_ref.clone();
        let download_results = download_results.clone();
        let is_downloading = is_downloading.clone();
        let download_progress = download_progress.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            is_downloading.set(true);
            download_progress.set("Starting download...".to_string());
            download_results.set(vec![]); // Clear previous results
            let value = greet_input_ref.cast::<web_sys::HtmlInputElement>().unwrap().value();
            log::info("home_download_clicked", serde_json::json!({ "url": value }));
            web_sys::console::log_1(&format!("Form submitted with URL: {}", value).into());
            wasm_bindgen_futures::spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "url": value })).unwrap();
                let _ = invoke("download_url", args).await;
            });
        })
    };

    let cancel_download = {
        let is_downloading = is_downloading.clone();
        let download_results = download_results.clone();
        Callback::from(move |_| {
            log::warn("home_download_cancel", serde_json::json!({}));
            is_downloading.set(false);
            download_results.set(vec![]);
            spawn_local(async {
                let _ = invoke("cancel_download", JsValue::NULL).await;
            });
        })
    };

    let open_click = {
        println!("[FRONTEND] [pages/home.rs] [open_click callback]");
        let on_open_file = props.on_open_file.clone();
        Callback::from(move |_| on_open_file.emit(()))
    };

    let ondragover = Callback::from(|e: DragEvent| {
        e.prevent_default();
        web_sys::console::log_1(&"Drag over".into());
    });

    let ondragleave = Callback::from(|e: DragEvent| {
        e.prevent_default();
        web_sys::console::log_1(&"Drag leave".into());
    });

    let ondrop = {
        println!("[FRONTEND] [pages/home.rs] [ondrop callback]");
        let on_csv_load = props.on_csv_load.clone();
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            web_sys::console::log_1(&"Drop event".into());
            if let Some(data) = e.data_transfer() {
                web_sys::console::log_1(&"Data transfer object found".into());
                if let Some(files) = data.files() {
                    web_sys::console::log_1(&format!("Files found: {}", files.length()).into());
                    if files.length() > 0 {
                        if let Some(file) = files.get(0) {
                            log::info("csv_drop_browser", serde_json::json!({ "filename": file.name() }));
                            web_sys::console::log_1(&format!("File name: {}", file.name()).into());
                            let file_reader = web_sys::FileReader::new().unwrap();
                            file_reader.read_as_text(&file).unwrap();
                            let on_csv_load = on_csv_load.clone();
                            let onload = Closure::wrap(Box::new(move |e: web_sys::ProgressEvent| {
                                web_sys::console::log_1(&"File loaded".into());
                                let reader: web_sys::FileReader = e.target().unwrap().dyn_into().unwrap();
                                let csv_text = reader.result().unwrap().as_string().unwrap();
                                log::info("csv_drop_loaded", serde_json::json!({ "bytes": csv_text.len() }));
                                on_csv_load.emit(csv_text);
                            }) as Box<dyn FnMut(_)>);
                            file_reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                            onload.forget();
                        }
                    }
                }
            }
        })
    };

    html! {
        <main class="container" {ondragover} {ondragleave} {ondrop}>
            <h1>{"Welcome to Clip Downloader"}</h1>
            <form class="home-form" onsubmit={greet}>
                <input id="url-input" ref={greet_input_ref} placeholder="Enter url..." oninput={on_input} disabled={*is_downloading} />
                { if !*is_downloading {
                    html! {
                        <button type="submit" class="download-cta" title="Download" disabled={!is_valid_url || *is_downloading}>
                            <Icon icon_id={IconId::LucideDownload} width={"36"} height={"36"} />
                        </button>
                    }
                } else {
                    html! {}
                }}
            </form>

            { if *is_downloading {
                html! {
                    <div class="row" style="margin-top: 16px; flex-direction: column; align-items: center; gap: 12px;">
                        <span style="font-family: monospace; font-size: 0.9em;">{(*download_progress).clone()}</span>
                        <button type="button" onclick={cancel_download}>{"Cancel"}</button>
                    </div>
                }
            } else {
                html! {}
            }}

            <div class="messages">
                { for (*download_results).clone().into_iter().map(|result| {
                    html! {
                        <div class={if result.success { "message-success" } else { "message-error" }}>
                            { result.message }
                        </div>
                    }
                })}
            </div>
            <div class="row home-actions">
                <button type="button" onclick={open_click}>{"Import list"}</button>
            </div>
        </main>
    }
}
