//! Web bindings. Build with `wasm-pack build crates/core-wasm --target web --out-dir ../../packages/core-wasm/pkg`.
//!
//! Mirrors `photos-core-uniffi` exactly (same JSON in / JSON out) so
//! `packages/core` can swap implementations per platform.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn version() -> String {
    photos_core::version().to_string()
}

#[wasm_bindgen]
pub struct Core {
    events: Vec<String>,
}

#[wasm_bindgen]
impl Core {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Core {
        Core { events: Vec::new() }
    }

    pub fn dispatch(&mut self, event_json: &str) -> Result<(), JsError> {
        serde_json::from_str::<serde_json::Value>(event_json)
            .map_err(|e| JsError::new(&format!("invalid event: {e}")))?;
        self.events.push(event_json.to_string());
        Ok(())
    }

    pub fn state_json(&self) -> String {
        serde_json::json!({ "version": photos_core::version(), "pending_events": self.events.len() })
            .to_string()
    }
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}
