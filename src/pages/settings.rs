use crate::dom::assign_missing_descriptive_ids;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_icons::{Icon, IconId};

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
struct SidecarCheck {
    yt_dlp: bool,
    gallery_dl: bool,
    ffmpeg: bool,
}

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
    #[serde(default)]
    pub use_system_binaries: bool,
    #[serde(default)]
    pub cooldown_secs: u32,
    #[serde(default)]
    pub retry_on_queue_empty: bool,
}

fn default_true() -> bool {
    true
}
fn default_parallel_downloads() -> u8 {
    3
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum DeleteMode {
    Soft,
    Hard,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum DefaultOutput {
    Audio,
    Video,
}

impl Default for DefaultOutput {
    fn default() -> Self {
        DefaultOutput::Video
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[function_component(SettingsPage)]
pub fn settings_page() -> Html {
    use_effect(|| {
        assign_missing_descriptive_ids("settings-page");
        || ()
    });
    let settings = use_state(Settings::default);
    let libs = use_state(|| None::<SidecarCheck>);
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
            let value = e
                .target_unchecked_into::<web_sys::HtmlSelectElement>()
                .value();
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
            let checked = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .checked();
            let mut s = (*settings).clone();
            s.debug_logs = checked;
            settings.set(s);
        })
    };

    let on_download_automatically_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .checked();
            let mut s = (*settings).clone();
            s.download_automatically = checked;
            settings.set(s);
        })
    };

    let on_keep_downloading_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .checked();
            let mut s = (*settings).clone();
            s.keep_downloading_on_other_pages = checked;
            settings.set(s);
        })
    };

    let on_parallel_downloads_change = {
        let settings = settings.clone();
        Callback::from(move |e: web_sys::InputEvent| {
            let value = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .value_as_number() as u8;
            let mut s = (*settings).clone();
            s.parallel_downloads = value.max(1);
            settings.set(s);
        })
    };

    let on_use_system_binaries_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .checked();
            let mut s = (*settings).clone();
            s.use_system_binaries = checked;
            settings.set(s);
        })
    };

    let on_cooldown_change = {
        let settings = settings.clone();
        Callback::from(move |e: web_sys::InputEvent| {
            let value = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .value_as_number() as u32;
            let mut s = (*settings).clone();
            s.cooldown_secs = value;
            settings.set(s);
        })
    };

    let on_retry_on_queue_empty_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let checked = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .checked();
            let mut s = (*settings).clone();
            s.retry_on_queue_empty = checked;
            settings.set(s);
        })
    };

    let on_delete_mode_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e
                .target_unchecked_into::<web_sys::HtmlSelectElement>()
                .value();
            let mut s = (*settings).clone();
            s.delete_mode = if value == "hard" {
                DeleteMode::Hard
            } else {
                DeleteMode::Soft
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

    let on_default_output_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let value = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .value();
            let mut s = (*settings).clone();
            s.default_output = if value == "audio" {
                DefaultOutput::Audio
            } else {
                DefaultOutput::Video
            };
            settings.set(s);
        })
    };

    let on_check_tools = {
        let libs = libs.clone();
        Callback::from(move |_| {
            // Clone outside the async move so the outer callback implements Fn
            let libs_set = libs.clone();
            spawn_local(async move {
                let v = invoke("check_sidecar_tools", JsValue::NULL).await;
                if let Ok(res) = serde_wasm_bindgen::from_value::<SidecarCheck>(v) {
                    libs_set.set(Some(res));
                }
            });
        })
    };

    html! {
        <main id="settings-page" class="container">
            <h1 id="settings-page-heading">{"Settings"}</h1>
            <div id="settings-form" class="settings-form">
                <div id="settings-download-directory-group" class="form-group">
                    <label id="settings-download-directory-label" for="settings-download-directory-input">{"Default Download Directory"}</label>
                    <div id="settings-download-directory-controls" class="input-group">
                        <input type="text" id="settings-download-directory-input" readonly=true value={settings.download_directory.clone()} />
                        <button id="settings-select-directory-button" onclick={on_directory_pick}>{"Select"}</button>
                        <button id="settings-open-directory-button" onclick={on_open_directory} class="icon-btn">
                            <Icon icon_id={IconId::LucideFolder} width={"30"} height={"30"} />
                        </button>
                    </div>
                </div>

                <div id="settings-duplicate-behavior-group" class="form-group row">
                    <label id="settings-duplicate-behavior-label" for="settings-duplicate-behavior-select">{"If duplicate name"}</label>
                    <select id="settings-duplicate-behavior-select" onchange={on_duplicate_change}>
                        <option id="settings-duplicate-behavior-create-new-option" value="CreateNew" selected={settings.on_duplicate == OnDuplicate::CreateNew}>{"Create new file"}</option>
                        <option id="settings-duplicate-behavior-overwrite-option" value="Overwrite" selected={settings.on_duplicate == OnDuplicate::Overwrite}>{"Overwrite file"}</option>
                        <option id="settings-duplicate-behavior-do-nothing-option" value="DoNothing" selected={settings.on_duplicate == OnDuplicate::DoNothing}>{"Do nothing"}</option>
                    </select>
                </div>

                <div id="settings-delete-mode-group" class="form-group row">
                    <label id="settings-delete-mode-label" for="settings-delete-mode-select">{"Delete behavior"}</label>
                    <select id="settings-delete-mode-select" onchange={on_delete_mode_change}>
                        <option id="settings-delete-mode-soft-option" value="Soft" selected={settings.delete_mode == DeleteMode::Soft}>
                            {"Soft delete (remove from library only)"}
                        </option>
                        <option id="settings-delete-mode-hard-option" value="Hard" selected={settings.delete_mode == DeleteMode::Hard}>
                            {"Hard delete (remove files from disk)"}
                        </option>
                    </select>
                </div>

                <div id="settings-default-output-group" class="form-group row">
                    <label id="settings-default-output-label">{"Default output"}</label>
                    <div id="settings-default-output-options" style="display:flex; gap: 16px; align-items:center;">
                        <label id="settings-default-output-audio-label" for="settings-default-output-audio-radio" style="display:flex; gap:6px; align-items:center;">
                            <input id="settings-default-output-audio-radio" type="radio" name="default-output" value="audio" onchange={on_default_output_change.clone()} checked={settings.default_output == DefaultOutput::Audio} />
                            {"Audio"}
                        </label>
                        <label id="settings-default-output-video-label" for="settings-default-output-video-radio" style="display:flex; gap:6px; align-items:center;">
                            <input id="settings-default-output-video-radio" type="radio" name="default-output" value="video" onchange={on_default_output_change} checked={settings.default_output == DefaultOutput::Video} />
                            {"Video"}
                        </label>
                    </div>
                </div>

                <div id="settings-debug-logs-group" class="form-group row">
                    <label id="settings-debug-logs-label" for="settings-debug-logs-checkbox">{"Activate debug logs"}</label>
                    <input type="checkbox" id="settings-debug-logs-checkbox" checked={settings.debug_logs} onchange={on_debug_logs_change} />
                </div>

                <div id="settings-download-automatically-group" class="form-group row">
                    <label id="settings-download-automatically-label" for="settings-download-automatically-checkbox">{"Download automatically"}</label>
                    <input type="checkbox" id="settings-download-automatically-checkbox" checked={settings.download_automatically} onchange={on_download_automatically_change} />
                </div>

                <div id="settings-keep-downloading-group" class="form-group row">
                    <label id="settings-keep-downloading-label" for="settings-keep-downloading-checkbox">{"Keep downloading on other pages"}</label>
                    <input type="checkbox" id="settings-keep-downloading-checkbox" checked={settings.keep_downloading_on_other_pages} onchange={on_keep_downloading_change} />
                </div>

                <div id="settings-parallel-downloads-group" class="form-group row">
                    <label id="settings-parallel-downloads-label" for="settings-parallel-downloads-input">{"Parallel downloads"}</label>
                    <input type="number" id="settings-parallel-downloads-input" min="1" value={settings.parallel_downloads.to_string()} oninput={on_parallel_downloads_change} />
                </div>

                <div id="settings-cooldown-group" class="form-group row">
                    <label id="settings-cooldown-label" for="settings-cooldown-input">{"Cooldown between downloads (seconds)"}</label>
                    <input type="number" id="settings-cooldown-input" min="0" value={settings.cooldown_secs.to_string()} oninput={on_cooldown_change} />
                </div>

                <div id="settings-retry-on-empty-group" class="form-group row">
                    <label id="settings-retry-on-empty-label" for="settings-retry-on-empty-checkbox">{"Retry failed downloads when queue empties"}</label>
                    <input type="checkbox" id="settings-retry-on-empty-checkbox" checked={settings.retry_on_queue_empty} onchange={on_retry_on_queue_empty_change} />
                </div>

                <div id="settings-local-libraries-group" class="form-group row">
                    <label id="settings-local-libraries-label">{"Check for local libraries"}</label>
                    <div id="settings-local-libraries-controls" style="display:flex; gap: 12px; align-items:center;">
                        <button id="settings-check-tools-button" onclick={on_check_tools}>{"Check"}</button>
                        {
                            if let Some(stats) = (*libs).clone() {
                                let ok_color = "#22c55e";
                                let bad_color = "#ef4444";
                                html!{
                                    <div id="settings-tool-status-summary" style="display:flex; gap: 16px; align-items:center; font-weight: 600;">
                                        <span id="settings-yt-dlp-status" style={format!("display:inline-flex; gap:6px; align-items:center; color:{};", if stats.yt_dlp { ok_color } else { bad_color })}>
                                            { if stats.yt_dlp { "✓" } else { "✗" } }{" yt-dlp"}
                                        </span>
                                        <span id="settings-gallery-dl-status" style={format!("display:inline-flex; gap:6px; align-items:center; color:{};", if stats.gallery_dl { ok_color } else { bad_color })}>
                                            { if stats.gallery_dl { "✓" } else { "✗" } }{" gallery-dl"}
                                        </span>
                                        <span id="settings-ffmpeg-status" style={format!("display:inline-flex; gap:6px; align-items:center; color:{};", if stats.ffmpeg { ok_color } else { bad_color })}>
                                            { if stats.ffmpeg { "✓" } else { "✗" } }{" ffmpeg"}
                                        </span>
                                    </div>
                                }
                            } else { html!{} }
                        }
                    </div>
                </div>

                {
                    if let Some(stats) = (*libs).clone() {
                        if stats.yt_dlp && stats.gallery_dl && stats.ffmpeg {
                            html!{
                                <div id="settings-system-binaries-group" class="form-group row">
                                    <label id="settings-system-binaries-label" for="settings-system-binaries-checkbox">{"Use local dependencies instead of sidecar"}</label>
                                    <input
                                        type="checkbox"
                                        id="settings-system-binaries-checkbox"
                                        checked={settings.use_system_binaries}
                                        onchange={on_use_system_binaries_change}
                                    />
                                </div>
                            }
                        } else { html!{} }
                    } else { html!{} }
                }

                <div id="settings-save-group" class="form-group center">
                    <button id="settings-save-button" onclick={on_save}>{"Save"}</button>
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
            use_system_binaries: false,
            cooldown_secs: 0,
            retry_on_queue_empty: false,
        }
    }
}
