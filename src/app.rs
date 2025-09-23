use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{FileReader, HtmlInputElement, ProgressEvent};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Tiktok,
    Instagram,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ContentType {
    Liked,
    Reposts,
    Profile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MediaKind {
    Pictures,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ClipRow {
    #[serde(rename = "Platform")]
    platform: Platform,
    #[serde(rename = "Type")]
    content_type: ContentType,
    #[serde(rename = "Handle")]
    handle: String,
    #[serde(rename = "Media")]
    media: MediaKind,
    #[serde(rename = "link")]
    link: String,
}

fn parse_and_log_csv(csv_text: &str) {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_text.as_bytes());

    for record in reader.deserialize::<ClipRow>() {
        match record {
            Ok(row) => {
                web_sys::console::log_1(&format!("{:?}", row).into());
            }
            Err(err) => {
                web_sys::console::error_1(&format!("CSV parse error: {err}").into());
            }
        }
    }
}

#[function_component(App)]
pub fn app() -> Html {
    let greet_input_ref = use_node_ref();
    let file_input_ref = use_node_ref();

    let name = use_state(|| String::new());

    let greet_msg = use_state(|| String::new());
    {
        let greet_msg = greet_msg.clone();
        let name = name.clone();
        let name2 = name.clone();
        use_effect_with(
            name2,
            move |_| {
                spawn_local(async move {
                    if name.is_empty() {
                        return;
                    }

                    let args = serde_wasm_bindgen::to_value(&GreetArgs { name: &*name }).unwrap();
                    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
                    let new_msg = invoke("greet", args).await.as_string().unwrap();
                    greet_msg.set(new_msg);
                });

                || {}
            },
        );
    }

    let greet = {
        let name = name.clone();
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            name.set(
                greet_input_ref
                    .cast::<web_sys::HtmlInputElement>()
                    .unwrap()
                    .value(),
            );
        })
    };

    let open_file_click = {
<<<<<<< HEAD
        Callback::from(move |_| {
            spawn_local(async move {
                // Use Tauri dialog to start at the home directory
                let csv_js = invoke("pick_csv_and_read", JsValue::NULL).await;
                if let Some(csv_text) = csv_js.as_string() {
                    parse_and_log_csv(&csv_text);
                }
            });
        })
    };

    // Removed: HTML input file flow in favor of Tauri dialog
=======
        let file_input_ref = file_input_ref.clone();
        Callback::from(move |_| {
            if let Some(input) = file_input_ref.cast::<HtmlInputElement>() {
                input.click();
            }
        })
    };

    let on_file_change = Callback::from(move |event: web_sys::Event| {
        let target = event.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
        if let Some(input) = target {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let reader = FileReader::new().unwrap();
                    let reader_clone = reader.clone();
                    let on_loadend = Closure::<dyn FnMut(ProgressEvent)>::new(move |_e| {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(text) = result.as_string() {
                                parse_and_log_csv(&text);
                            }
                        }
                    });
                    reader.set_onloadend(Some(on_loadend.as_ref().unchecked_ref()));
                    on_loadend.forget();
                    let _ = reader.read_as_text(&file);
                }
            }
        }
    });
>>>>>>> 2f9a086 (feature/open-csv)

    html! {
        <main class="container">
            <h1>{"Welcome to Tauri + Yew"}</h1>

            <div class="row">
                <a href="https://tauri.app" target="_blank">
                    <img src="public/tauri.svg" class="logo tauri" alt="Tauri logo"/>
                </a>
                <a href="https://yew.rs" target="_blank">
                    <img src="public/yew.png" class="logo yew" alt="Yew logo"/>
                </a>
            </div>
            <p>{"Click on the Tauri and Yew logos to learn more."}</p>

            <form class="row" onsubmit={greet}>
                <input id="greet-input" ref={greet_input_ref} placeholder="Enter a name..." />
                <button type="submit">{"Greet"}</button>
            </form>
            <p>{ &*greet_msg }</p>

            <div class="row">
<<<<<<< HEAD
=======
                <input
                    ref={file_input_ref}
                    type="file"
                    accept=".csv,text/csv"
                    style="display: none;"
                    onchange={on_file_change}
                />
>>>>>>> 2f9a086 (feature/open-csv)
                <button type="button" onclick={open_file_click}>{"Open file"}</button>
            </div>
        </main>
    }
}
