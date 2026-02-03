use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/main.js")]
extern "C" {
    pub fn push_notification(notification: String);
}
