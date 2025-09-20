#[cfg(target_arch = "wasm32")]
mod app;

#[cfg(target_arch = "wasm32")]
use app::App;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    yew::Renderer::<App>::new().render();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "This crate targets WebAssembly. Use `tauri dev` (runs `trunk serve`) or `trunk serve` to run the frontend. Do not `cargo run` this crate natively."
    );
    std::process::exit(1);
}
