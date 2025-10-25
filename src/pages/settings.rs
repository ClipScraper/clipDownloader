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
    pub id: Option<i64>,
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
    pub delete_mode: DeleteMode,
    pub debug_logs: bool,
    #[serde(default)]
    pub default_output: DefaultOutput,
    #[serde(default = "default_true")]
    pub download_automatically: bool,
    #[serde(default = "default_true")]
    pub keep_downloading_on_other_pages: bool,
    #[serde(default = "default_parallel_downloads")]
    pub parallel_downloads: u8,
}

fn default_true() -> bool { true }
fn default_parallel_downloads() -> u8 { 3 }

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum DeleteMode { Soft, Hard }

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum DefaultOutput { Audio, Video }

impl Default for DefaultOutput {
    fn default() -> Self { DefaultOutput::Video }
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

    let on_debug_logs_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e.target_unchecked_into::<web_sys::HtmlInputElement>().checked();
            let mut s = (*settings).clone();
            s.debug_logs = checked;
            settings.set(s);
        })
    };

    let on_download_automatically_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e.target_unchecked_into::<web_sys::HtmlInputElement>().checked();
            let mut s = (*settings).clone();
            s.download_automatically = checked;
            settings.set(s);
        })
    };

    let on_keep_downloading_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e.target_unchecked_into::<web_sys::HtmlInputElement>().checked();
            let mut s = (*settings).clone();
            s.keep_downloading_on_other_pages = checked;
            settings.set(s);
        })
    };

    let on_parallel_downloads_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e.target_unchecked_into::<web_sys::HtmlInputElement>().value_as_number() as u8;
            let mut s = (*settings).clone();
            s.parallel_downloads = value.max(1);
            settings.set(s);
        })
    };

    let on_delete_mode_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e.target_unchecked_into::<web_sys::HtmlSelectElement>().value();
            let mut s = (*settings).clone();
            s.delete_mode = if value == "hard" { DeleteMode::Hard } else { DeleteMode::Soft };
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

    let on_default_output_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e.target_unchecked_into::<web_sys::HtmlInputElement>().value();
            let mut s = (*settings).clone();
            s.default_output = if value == "audio" { DefaultOutput::Audio } else { DefaultOutput::Video };
            settings.set(s);
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
                        <option value="CreateNew" selected={settings.on_duplicate == OnDuplicate::CreateNew}>{"Create new file"}</option>
                        <option value="Overwrite" selected={settings.on_duplicate == OnDuplicate::Overwrite}>{"Overwrite file"}</option>
                        <option value="DoNothing" selected={settings.on_duplicate == OnDuplicate::DoNothing}>{"Do nothing"}</option>
                    </select>
                </div>

                <div class="form-group row">
                    <label for="delete-mode">{"Delete behavior"}</label>
                    <select id="delete-mode" onchange={on_delete_mode_change}>
                        <option value="Soft" selected={settings.delete_mode == DeleteMode::Soft}>
                            {"Soft delete (remove from library only)"}
                        </option>
                        <option value="Hard" selected={settings.delete_mode == DeleteMode::Hard}>
                            {"Hard delete (remove files from disk)"}
                        </option>
                    </select>
                </div>

                <div class="form-group row">
                    <label>{"Default output"}</label>
                    <div style="display:flex; gap: 16px; align-items:center;">
                        <label style="display:flex; gap:6px; align-items:center;">
                            <input type="radio" name="default-output" value="audio" onchange={on_default_output_change.clone()} checked={settings.default_output == DefaultOutput::Audio} />
                            {"Audio"}
                        </label>
                        <label style="display:flex; gap:6px; align-items:center;">
                            <input type="radio" name="default-output" value="video" onchange={on_default_output_change} checked={settings.default_output == DefaultOutput::Video} />
                            {"Video"}
                        </label>
                    </div>
                </div>

                <div class="form-group row">
                    <label for="debug-logs">{"Activate debug logs"}</label>
                    <input type="checkbox" id="debug-logs" checked={settings.debug_logs} onchange={on_debug_logs_change} />
                </div>

                <div class="form-group row">
                    <label for="download-automatically">{"Download automatically"}</label>
                    <input type="checkbox" id="download-automatically" checked={settings.download_automatically} onchange={on_download_automatically_change} />
                </div>

                <div class="form-group row">
                    <label for="keep-downloading">{"Keep downloading on other pages"}</label>
                    <input type="checkbox" id="keep-downloading" checked={settings.keep_downloading_on_other_pages} onchange={on_keep_downloading_change} />
                </div>

                <div class="form-group row">
                    <label for="parallel-downloads">{"Parallel downloads"}</label>
                    <input type="number" id="parallel-downloads" min="1" value={settings.parallel_downloads.to_string()} onchange={on_parallel_downloads_change} />
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
            id: None,
            download_directory: String::from(""),
            on_duplicate: OnDuplicate::CreateNew,
            delete_mode: DeleteMode::Soft,
            debug_logs: false,
            default_output: DefaultOutput::Video,
            download_automatically: true,
            keep_downloading_on_other_pages: true,
            parallel_downloads: 3,
        }
    }
}

