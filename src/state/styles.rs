use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;
use crate::styles::{self, KeyStyle};

/// Thread-safe registry of available key styles (built-in + user JSON files).
pub struct StylesState {
    inner: ArcSwap<Vec<KeyStyle>>,
}

impl StylesState {
    /// Load built-in styles and scan the user styles directory.
    pub fn load() -> Self {
        let mut all = styles::builtins();
        all.extend(load_user_styles());
        info!("Loaded {} key style(s)", all.len());
        Self {
            inner: ArcSwap::from_pointee(all),
        }
    }

    /// Snapshot of all available styles.
    #[allow(dead_code)]
    pub fn snapshot(&self) -> Arc<Vec<KeyStyle>> {
        self.inner.load_full()
    }

    /// Look up a style by ID.  Returns a clone.
    pub fn get(&self, id: &str) -> Option<KeyStyle> {
        self.inner.load().iter().find(|s| s.id == id).cloned()
    }

    /// List of (id, display_name) pairs for PI dropdowns.
    pub fn list(&self) -> Vec<(String, String)> {
        self.inner
            .load()
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect()
    }

    /// Re-scan the user styles directory and merge with built-ins.
    #[allow(dead_code)]
    pub fn reload(&self) {
        let mut all = styles::builtins();
        all.extend(load_user_styles());
        info!("Reloaded {} key style(s)", all.len());
        self.inner.store(Arc::new(all));
    }
}

/// Resolve the effective style for a key.
///
/// Priority: per-key `key_style` → global `defaultKeyStyle` → built-in default.
pub fn resolve_style(
    per_key_id: &str,
    styles: &StylesState,
    globals: &streamdeck_lib::prelude::GlobalSettings,
) -> KeyStyle {
    // 1. Per-key override
    if !per_key_id.is_empty()
        && let Some(s) = styles.get(per_key_id)
    {
        return s;
    }

    // 2. Global default
    if let Some(id) = globals
        .get("defaultKeyStyle")
        .and_then(|v| v.as_str().map(String::from))
        && !id.is_empty()
        && let Some(s) = styles.get(&id)
    {
        return s;
    }

    // 3. Built-in default
    styles::style_default()
}

// ── User Style Loading ───────────────────────────────────────────────────────

/// Returns the user styles directory: `%APPDATA%/icu.veelume.starcitizen/styles/`
fn user_styles_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(PLUGIN_ID).join("styles"))
}

/// Load all `.json` files from the user styles directory.
fn load_user_styles() -> Vec<KeyStyle> {
    let Some(dir) = user_styles_dir() else {
        return Vec::new();
    };

    if !dir.is_dir() {
        debug!("User styles directory does not exist: {}", dir.display());
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&dir) else {
        warn!("Failed to read user styles directory: {}", dir.display());
        return Vec::new();
    };

    let mut styles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        match load_style_file(&path) {
            Ok(mut style) => {
                // Use filename stem as ID if the file doesn't set one
                if style.id.is_empty()
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    style.id = stem.to_string();
                }
                // Use ID as display name if name is empty
                if style.name.is_empty() {
                    style.name = style.id.clone();
                }
                info!("Loaded user style '{}' from {}", style.id, path.display());
                styles.push(style);
            }
            Err(e) => {
                warn!("Failed to load style from {}: {e}", path.display());
            }
        }
    }

    styles
}

fn load_style_file(path: &std::path::Path) -> anyhow::Result<KeyStyle> {
    let contents = std::fs::read_to_string(path)?;
    let style: KeyStyle = serde_json::from_str(&contents)?;
    Ok(style)
}
