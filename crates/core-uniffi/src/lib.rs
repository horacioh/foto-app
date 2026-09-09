//! Mobile bindings. Generate Swift/Kotlin with:
//!
//! ```sh
//! cargo build -p photos-core-uniffi --release
//! cargo run -p photos-core-uniffi --features bindgen --bin uniffi-bindgen -- \
//!   generate --library target/release/libphotos_core_ffi.so --language swift --out-dir packages/core-native/ios/generated
//! ```
//!
//! The surface is intentionally tiny: the UI talks to the core through
//! `dispatch(event_json)` and `subscribe`-style polling of `state_json()`. Both
//! are JSON so the TypeScript facade in `packages/core` can share one type
//! layer between native and web (ARCHITECTURE.md §7, "Actor ↔ UI bridge").

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn version() -> String {
    photos_core::version().to_string()
}

/// Handle to a running core. Phase 1 wires this to the supervisor; for now it
/// echoes events so the bindings pipeline can be exercised end to end.
#[derive(uniffi::Object)]
pub struct Core {
    events: std::sync::Mutex<Vec<String>>,
}

#[uniffi::export]
impl Core {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self { events: std::sync::Mutex::new(Vec::new()) }
    }

    /// Accepts a JSON-encoded UI event. Returns an error string if the JSON is invalid.
    pub fn dispatch(&self, event_json: String) -> Result<(), CoreError> {
        serde_json::from_str::<serde_json::Value>(&event_json)
            .map_err(|e| CoreError::InvalidEvent { message: e.to_string() })?;
        self.events.lock().expect("core mutex").push(event_json);
        Ok(())
    }

    /// JSON snapshot of the state the UI renders.
    pub fn state_json(&self) -> String {
        let events = self.events.lock().expect("core mutex");
        serde_json::json!({ "version": photos_core::version(), "pending_events": events.len() })
            .to_string()
    }
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("invalid event: {message}")]
    InvalidEvent { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_validates_json() {
        let core = Core::new();
        assert!(core.dispatch("{\"type\":\"ping\"}".into()).is_ok());
        assert!(core.dispatch("nope".into()).is_err());
        assert!(core.state_json().contains("\"pending_events\":1"));
    }
}
