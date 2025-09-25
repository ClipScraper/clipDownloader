use wasm_bindgen::prelude::*;
use yew::prelude::*;

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
    let greet_msg = use_state(|| String::new());

    {
        let greet_msg = greet_msg.clone();
        let name = name.clone();
        let name2 = name.clone();
        use_effect_with(name2, move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if name.is_empty() { return; }
                let args = serde_wasm_bindgen::to_value(&GreetArgs { name: &*name }).unwrap();
                let new_msg = invoke("greet", args).await.as_string().unwrap();
                greet_msg.set(new_msg);
            });
            || {}
        });
    }

    let greet = {
        let name = name.clone();
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            name.set(greet_input_ref.cast::<web_sys::HtmlInputElement>().unwrap().value());
        })
    };

    let open_click = {
        let on_open_file = props.on_open_file.clone();
        Callback::from(move |_| on_open_file.emit(()))
    };

    html! {
        <main class="container">
            <h1>{"Welcome to Clip Downloader"}</h1>
            <form class="row" onsubmit={greet}>
                <input id="greet-input" ref={greet_input_ref} placeholder="Enter url..." />
                <button type="submit">{"Download"}</button>
            </form>
            <p>{ &*greet_msg }</p>
            <div class="row">
                <button type="button" onclick={open_click}>{"Open file"}</button>
            </div>
        </main>
    }
}

