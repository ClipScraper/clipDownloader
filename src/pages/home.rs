use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::DragEvent;
use yew::prelude::*;
use serde::{Serialize, Deserialize};
use yew_hooks::prelude::*;

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
    let greet_input_ref = use_node_ref();
    let name = use_state(|| String::new());
    let download_results = use_state(|| Vec::<DownloadResult>::new());

    {
        let download_results = download_results.clone();
        use_effect_once(move || {
            let download_results = download_results.clone();
            spawn_local(async move {
                let closure = Closure::wrap(Box::new(move |event: JsValue| {
                    if let Ok(result) = serde_wasm_bindgen::from_value(event) {
                        let mut results = (*download_results).clone();
                        results.push(result);
                        download_results.set(results);
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
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            download_results.set(vec![]); // Clear previous results
            let value = greet_input_ref.cast::<web_sys::HtmlInputElement>().unwrap().value();
            web_sys::console::log_1(&format!("Form submitted with URL: {}", value).into());
            wasm_bindgen_futures::spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "url": value })).unwrap();
                let _ = invoke("download_url", args).await;
            });
        })
    };

    let open_click = {
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
                            web_sys::console::log_1(&format!("File name: {}", file.name()).into());
                            let file_reader = web_sys::FileReader::new().unwrap();
                            file_reader.read_as_text(&file).unwrap();
                            let on_csv_load = on_csv_load.clone();
                            let onload = Closure::wrap(Box::new(move |e: web_sys::ProgressEvent| {
                                web_sys::console::log_1(&"File loaded".into());
                                let reader: web_sys::FileReader = e.target().unwrap().dyn_into().unwrap();
                                let csv_text = reader.result().unwrap().as_string().unwrap();
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
            <form class="row home-form" onsubmit={greet}>
                <input id="url-input" ref={greet_input_ref} placeholder="Enter url..." oninput={on_input} />
                { if name.contains("instagram.com") || name.contains("tiktok.com") || name.contains("youtube.com") || name.contains("youtu.be") {
                    html!{ <button type="submit" class="download-cta" title="Download"><img class="download-icon" src="assets/download.svg" /></button> }
                } else { html!{} } }
            </form>
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
