use yew::prelude::*;

#[function_component(ExtensionPage)]
pub fn extension_page() -> Html {
    html! {
        <div class="container">
            <h1>{"Extension"}</h1>
            <p>{"Information about the browser extension will go here."}</p>
        </div>
    }
}
