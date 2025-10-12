use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

// Bind to window.__TAURI__.core.invoke under a different Rust name to avoid symbol clashes
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window","__TAURI__","core"], js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

fn send(level: &str, message: &str, context: Value) {
    let lvl = level.to_string();
    let msg = message.to_string();
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&json!({
            "level": lvl,
            "message": msg,
            "context": context
        }))
        .unwrap();
        let _ = tauri_invoke("frontend_log", args).await;
    });
}

pub fn info(message: &str, context: Value)  { send("info",  message, context); }
pub fn warn(message: &str, context: Value)  { send("warn",  message, context); }
pub fn error(message: &str, context: Value) { send("error", message, context); }
pub fn debug(message: &str, context: Value) { send("debug", message, context); }
