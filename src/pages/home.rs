use wasm_bindgen::prelude::*;
use yew::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> { name: &'a str }

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Properties, PartialEq, Clone)]
pub struct Props {
    pub on_open_file: Callback<()>,
}

#[function_component(HomePage)]
pub fn home_page(props: &Props) -> Html {
    let greet_input_ref = use_node_ref();
    let name = use_state(|| String::new());
    let on_input = {
        let name = name.clone();
        Callback::from(move |e: web_sys::InputEvent| {
            if let Some(t) = e.target() { if let Ok(inp) = t.dyn_into::<web_sys::HtmlInputElement>() { name.set(inp.value()); } }
        })
    };
    let greet = {
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let value = greet_input_ref.cast::<web_sys::HtmlInputElement>().unwrap().value();
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

    html! {
        <main class="container">
            <h1>{"Welcome to Clip Downloader"}</h1>
            <form class="row home-form" onsubmit={greet}>
                <input id="url-input" ref={greet_input_ref} placeholder="Enter url..." oninput={on_input} />
                { if name.contains("instagram.com") || name.contains("tiktok.com") || name.contains("youtube.com") || name.contains("youtu.be") {
                    html!{ <button type="submit" class="download-cta" title="Download"><img class="download-icon" src="assets/download.svg" /></button> }
                } else { html!{} } }
            </form>
            <div class="row home-actions">
                <button type="button" onclick={open_click}>{"Import list"}</button>
            </div>
        </main>
    }
}

