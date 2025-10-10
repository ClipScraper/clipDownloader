use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_icons::{Icon, IconId};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum OnDuplicate {
    Overwrite,
    CreateNew,
    DoNothing,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[function_component(SettingsPage)]
pub fn settings_page() -> Html {
    let settings = use_state(Settings::default);
    let settings_clone = settings.clone();
    use_effect_with((), move |_| {
        spawn_local(async move {
            let loaded_settings = invoke("load_settings", JsValue::NULL).await;
            if let Ok(s) = serde_wasm_bindgen::from_value(loaded_settings) {
                settings_clone.set(s);
            }
        });
    });

    let on_directory_pick = {
        let settings = settings.clone();
        Callback::from(move |_| {
            let settings = settings.clone();
            spawn_local(async move {
                let result = invoke("pick_directory", JsValue::NULL).await;
                if let Some(path) = result.as_string() {
                    let mut s = (*settings).clone();
                    s.download_directory = path;
                    settings.set(s);
                }
            });
        })
    };

    let on_open_directory = {
        let settings = settings.clone();
        Callback::from(move |_| {
            let path = settings.download_directory.clone();
            spawn_local(async move {
                let result = invoke(
                    "open_directory",
                    serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path })).unwrap(),
                )
                .await;
                if !result.is_null() {
                    web_sys::console::error_1(&"Failed to open directory:".into());
                    web_sys::console::error_1(&result);
                }
            });
        })
    };

    let on_duplicate_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e.target_unchecked_into::<web_sys::HtmlSelectElement>().value();
            let mut s = (*settings).clone();
            s.on_duplicate = match value.as_str() {
                "overwrite" => OnDuplicate::Overwrite,
                "do_nothing" => OnDuplicate::DoNothing,
                _ => OnDuplicate::CreateNew,
            };
            settings.set(s);
        })
    };

    let on_save = {
        let settings = settings.clone();
        Callback::from(move |_| {
            let settings_to_save = (*settings).clone();
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(
                    &serde_json::json!({ "settings": settings_to_save }),
                )
                .unwrap();
                let result = invoke("save_settings", args).await;
                if result.is_null() {
                    web_sys::console::log_1(&"Settings saved successfully.".into());
                } else {
                    web_sys::console::error_1(&"Failed to save settings:".into());
                    web_sys::console::error_1(&result);
                }
            });
        })
    };

    html! {
        <main class="container">
            <h1>{"Settings"}</h1>
            <div class="settings-form">
                <div class="form-group">
                    <label for="download-dir">{"Default Download Directory"}</label>
                    <div class="input-group">
                        <input type="text" id="download-dir" readonly=true value={settings.download_directory.clone()} />
                        <button onclick={on_directory_pick}>{"Browse"}</button>
                        <button onclick={on_open_directory} class="icon-btn">
                            <Icon icon_id={IconId::LucideFolder} width={"30"} height={"30"} />
                        </button>
                    </div>
                </div>

                <div class="form-group row">
                    <label for="on-duplicate">{"If duplicate name"}</label>
                    <select id="on-duplicate" onchange={on_duplicate_change}>
                        <option value="create_new" selected={settings.on_duplicate == OnDuplicate::CreateNew}>{"Create new file"}</option>
                        <option value="overwrite" selected={settings.on_duplicate == OnDuplicate::Overwrite}>{"Overwrite file"}</option>
                        <option value="do_nothing" selected={settings.on_duplicate == OnDuplicate::DoNothing}>{"Do nothing"}</option>
                    </select>
                </div>
                <div class="form-group center">
                    <button onclick={on_save}>{"Save"}</button>
                </div>
            </div>
        </main>
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_directory: String::from(""),
            on_duplicate: OnDuplicate::CreateNew,
        }
    }
}

