//! Saving settings and progress: a key-value store of strings (JSON text, usually) that each
//! host implements its own way. The app: files next to the executable natively, `localStorage`
//! in the browser; the tools: a temporary folder; tests: memory. Games never touch files.

use std::collections::BTreeMap;

/// Strings by key. Keys are short identifiers ("blacktap_settings").
pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn remove(&mut self, key: &str);
}

/// A store in memory (tests, and hosts that can't save).
#[derive(Clone, Debug, Default)]
pub struct MemoryStore(pub BTreeMap<String, String>);

impl Store for MemoryStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }
    fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
}

/// Reads `key` as `T`; a missing or corrupt value gives the default and, when it was corrupt,
/// the parse error (so the host can say so: "settings were reset").
pub fn load<T: serde::de::DeserializeOwned + Default>(store: &dyn Store, key: &str) -> (T, Option<String>) {
    match store.get(key) {
        None => (T::default(), None),
        Some(text) => match serde_json::from_str(&text) {
            Ok(v) => (v, None),
            Err(e) => (T::default(), Some(format!("{key}: {e}"))),
        },
    }
}

/// Writes `value` as JSON under `key`.
pub fn save<T: serde::Serialize>(store: &mut dyn Store, key: &str, value: &T) {
    if let Ok(text) = serde_json::to_string(value) {
        store.set(key, &text);
    }
}
