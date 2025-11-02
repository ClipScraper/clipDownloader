use yew::prelude::*;
use wasm_bindgen::prelude::*;
use gloo_timers::callback::Timeout;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[function_component(SponsorPage)]
pub fn sponsor_page() -> Html {
    let recently_copied = use_state(|| String::new());

    let make_copy_callback = |address: &'static str, id: &'static str, recently_copied: UseStateHandle<String>| {
        Callback::from(move |_| {
            let recently_copied = recently_copied.clone();
            let id_str = id.to_string();
            let address_str = address.to_string();
            wasm_bindgen_futures::spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "text": address_str })).unwrap();
                invoke("plugin:clipboard|write_text", args).await;
            });
            recently_copied.set(id_str);
            let recently_copied_clone = recently_copied.clone();
            let timeout = Timeout::new(1500, move || {
                recently_copied_clone.set(String::new());
            });
            timeout.forget();
        })
    };

    let btc_addr = "bc1q99v9y7kt9ayu4r4ftxk2znzdcq5ca9fv988m5q";
    let eth_addr = "0x66994e0929576881B752a2BB8C249c9C8e74C253";

    html! {
        <div class="container">
            <h1>{"Support ClipScraper Development"}</h1>
            <p>{"If you find ClipScraper useful, or if there are any features you would like to see, please consider supporting its development."}</p>

            <div class="donation-addresses">
                <div class="address">
                    <span class="label">{"BTC:"}</span>
                    <span class="value">{btc_addr}</span>
                    <i class={classes!("copy-icon", "fas", if *recently_copied == "btc" { "fa-check" } else { "fa-copy" })} onclick={make_copy_callback(btc_addr, "btc", recently_copied.clone())} title="Copy address"></i>
                </div>
                <div class="address">
                    <span class="label">{"ETH (ERC20):"}</span>
                    <span class="value">{eth_addr}</span>
                    <i class={classes!("copy-icon", "fas", if *recently_copied == "eth" { "fa-check" } else { "fa-copy" })} onclick={make_copy_callback(eth_addr, "eth", recently_copied.clone())} title="Copy address"></i>
                </div>
                <div class="address">
                    <span class="label">{"USDT (ERC20):"}</span>
                    <span class="value">{eth_addr}</span>
                    <i class={classes!("copy-icon", "fas", if *recently_copied == "usdt" { "fa-check" } else { "fa-copy" })} onclick={make_copy_callback(eth_addr, "usdt", recently_copied.clone())} title="Copy address"></i>
                </div>
            </div>

            <p class="footer-note">{"If there are any other ways you would like to support the project, please let us know at support@clipscraper.com"}</p>
        </div>
    }
}
