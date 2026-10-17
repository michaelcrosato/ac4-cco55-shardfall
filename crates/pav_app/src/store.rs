//! `pav_core::persist::Store` for the app: one JSON file per key next to the executable
//! natively (`<key>.json`, written atomically), the browser's `localStorage` on the web.

pub struct AppStore;

#[cfg(not(target_arch = "wasm32"))]
fn path(key: &str) -> std::path::PathBuf {
    let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    crate::boot::exe_dir().join(format!("{safe}.json"))
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

impl pav_core::persist::Store for AppStore {
    fn get(&self, key: &str) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        return std::fs::read_to_string(path(key)).ok();
        #[cfg(target_arch = "wasm32")]
        return storage()?.get_item(&key.replace('_', ".")).ok()?;
    }

    fn set(&mut self, key: &str, value: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let p = path(key);
            let tmp = p.with_extension("json.tmp");
            if let Err(e) = std::fs::write(&tmp, value).and_then(|_| std::fs::rename(&tmp, &p)) {
                log::warn!("could not save {}: {e}", p.display());
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = storage() {
            let _ = s.set_item(&key.replace('_', "."), value);
        }
    }

    fn remove(&mut self, key: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::fs::remove_file(path(key));
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = storage() {
            let _ = s.remove_item(&key.replace('_', "."));
        }
    }
}
