use wasm_bindgen::prelude::*;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "shell"])]
    async fn open(url: &str);
}

#[function_component(ExtensionPage)]
pub fn extension_page() -> Html {
    let on_chrome_download_click = Callback::from(|e: MouseEvent| {
        e.prevent_default();
        let url = "https://chromewebstore.google.com/detail/listr/dogifpgpdjhldninabaejghgojpdokmn";
        wasm_bindgen_futures::spawn_local(async move {
            open(url).await;
        });
    });

    html! {
        <div class="container">
            <h1>{ "Choose Your Platform" }</h1>
            <section class="download-options">
                <div class="platform-grid">
                    <div class="platform-card">
                        <div class="platform-icon"><i class="fab fa-chrome"></i></div>
                        <h3>{ "Chrome" }</h3>
                        <p>{ "Chrome, Brave, Edge, and other Chromium browsers" }</p>
                        <a href="https://chromewebstore.google.com/detail/listr/dogifpgpdjhldninabaejghgojpdokmn"
                           target="_blank"
                           class="download-btn primary"
                           onclick={on_chrome_download_click}
                        >
                            { "Download for Chrome" }
                        </a>
                    </div>

                    <div class="platform-card coming-soon">
                        <div class="platform-icon"><i class="fab fa-firefox-browser"></i></div>
                        <h3>{ "Firefox" }</h3>
                        <p>{ "Mozilla Firefox" }</p>
                        <button class="download-btn primary" disabled={true}>
                            { "Coming Soon" }
                        </button>
                        <small>{ " " }</small>
                    </div>

                    <div class="platform-card coming-soon">
                        <div class="platform-icon"><i class="fab fa-safari"></i></div>
                        <h3>{ "Safari" }</h3>
                        <p>{ "Apple Safari" }</p>
                        <button class="download-btn primary" disabled={true}>
                            { "Coming Soon" }
                        </button>
                        <small>{ " " }</small>
                    </div>
                </div>
            </section>
        </div>
    }
}
