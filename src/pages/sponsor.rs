use yew::prelude::*;

#[function_component(SponsorPage)]
pub fn sponsor_page() -> Html {
    html! {
        <div class="container">
            <h1>{"Support ClipScraper Development"}</h1>
            <p>{"If you find ClipScraper useful, or if there are any features you would like to see, please consider supporting its development."}</p>

            <div class="donation-addresses">
                <div class="address">
                    <span class="label">{"BTC:"}</span>
                    <span class="value">{"bc1q99v9y7kt9ayu4r4ftxk2znzdcq5ca9fv988m5q"}</span>
                </div>
                <div class="address">
                    <span class="label">{"ETH (ERC20):"}</span>
                    <span class="value">{"0x66994e0929576881B752a2BB8C249c9C8e74C253"}</span>
                </div>
                <div class="address">
                    <span class="label">{"USDT (ERC20):"}</span>
                    <span class="value">{"0x66994e0929576881B752a2BB8C249c9C8e74C253"}</span>
                </div>
            </div>

            <p class="footer-note">{"If there are any other ways you would like to support the project, please let us know at support@clipscraper.com"}</p>
        </div>
    }
}
