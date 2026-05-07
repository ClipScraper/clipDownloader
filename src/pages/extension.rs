use crate::dom::assign_missing_descriptive_ids;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "shell"])]
    async fn open(url: &str);
}

#[function_component(ExtensionPage)]
pub fn extension_page() -> Html {
    use_effect(|| {
        assign_missing_descriptive_ids("extension-page");
        || ()
    });
    let on_chrome_download_click = Callback::from(|e: MouseEvent| {
        e.prevent_default();
        let url = "https://chromewebstore.google.com/detail/listr/dogifpgpdjhldninabaejghgojpdokmn";
        wasm_bindgen_futures::spawn_local(async move {
            open(url).await;
        });
    });

    html! {
        <div id="extension-page" class="container">
            <h1 id="extension-page-heading">{ "Choose Your Platform" }</h1>
            <section id="extension-download-options" class="download-options">
                <div id="extension-platform-grid" class="platform-grid">
                    <div id="extension-chrome-card" class="platform-card">
                        <div id="extension-chrome-icon-container" class="platform-icon"><i id="extension-chrome-icon" class="fab fa-chrome"></i></div>
                        <h3 id="extension-chrome-heading">{ "Chrome" }</h3>
                        <p id="extension-chrome-description">{ "Chrome, Brave, Edge, and other Chromium browsers" }</p>
                        <a href="https://chromewebstore.google.com/detail/listr/dogifpgpdjhldninabaejghgojpdokmn"
                           target="_blank"
                           id="extension-chrome-download-link"
                           class="download-btn primary"
                           onclick={on_chrome_download_click}
                        >
                            { "Download for Chrome" }
                        </a>
                    </div>

                    <div id="extension-firefox-card" class="platform-card coming-soon">
                        <div id="extension-firefox-icon-container" class="platform-icon"><i id="extension-firefox-icon" class="fab fa-firefox-browser"></i></div>
                        <h3 id="extension-firefox-heading">{ "Firefox" }</h3>
                        <p id="extension-firefox-description">{ "Mozilla Firefox" }</p>
                        <button id="extension-firefox-download-button" class="download-btn primary" disabled={true}>
                            { "Coming Soon" }
                        </button>
                        <small id="extension-firefox-note">{ " " }</small>
                    </div>

                    <div id="extension-safari-card" class="platform-card coming-soon">
                        <div id="extension-safari-icon-container" class="platform-icon"><i id="extension-safari-icon" class="fab fa-safari"></i></div>
                        <h3 id="extension-safari-heading">{ "Safari" }</h3>
                        <p id="extension-safari-description">{ "Apple Safari" }</p>
                        <button id="extension-safari-download-button" class="download-btn primary" disabled={true}>
                            { "Coming Soon" }
                        </button>
                        <small id="extension-safari-note">{ " " }</small>
                    </div>
                </div>
            </section>
        </div>
    }
}
