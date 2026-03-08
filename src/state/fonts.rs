use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use streamdeck_render::{FontHandle, FontRegistry};
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;

// ── Embedded Default ────────────────────────────────────────────────────────

static EMBEDDED_FONT: std::sync::OnceLock<FontHandle> = std::sync::OnceLock::new();

/// Load (or return cached) the compile-time embedded default font.
///
/// This is the single source of truth for the default font bytes — both
/// `FontsState` and the fallback path in `render.rs` use this.
pub fn embedded_font() -> FontHandle {
    EMBEDDED_FONT
        .get_or_init(|| {
            let mut reg = FontRegistry::new();
            reg.load_bytes(
                "default",
                include_bytes!(
                    "../../icu.veelume.starcitizen.sdPlugin/fonts/UAV-OSD-Sans-Mono.ttf"
                ),
            )
            .expect("embedded font must load")
        })
        .clone()
}

// ── Font Map ────────────────────────────────────────────────────────────────

/// Internal storage: id → (display_name, handle).
struct FontMap {
    handles: HashMap<String, FontHandle>,
    /// Ordered list of (id, display_name) for PI dropdowns.
    names: Vec<(String, String)>,
}

impl FontMap {
    fn new() -> Self {
        Self {
            handles: HashMap::new(),
            names: Vec::new(),
        }
    }

    fn insert(
        &mut self,
        id: impl Into<String>,
        display_name: impl Into<String>,
        handle: FontHandle,
    ) {
        let id = id.into();
        let display_name = display_name.into();
        self.handles.insert(id.clone(), handle);
        self.names.push((id, display_name));
    }

    fn get(&self, id: &str) -> Option<FontHandle> {
        self.handles.get(id).cloned()
    }
}

// ── FontsState ──────────────────────────────────────────────────────────────

/// Thread-safe registry of available fonts (embedded default + user font files).
pub struct FontsState {
    inner: ArcSwap<FontMap>,
    default: FontHandle,
}

impl FontsState {
    /// Load the embedded default font and scan the user fonts directory.
    pub fn load() -> Self {
        let default = embedded_font();

        let mut map = FontMap::new();
        map.insert("default", "Default (Mono)", default.clone());
        load_user_fonts(&mut map);

        info!("Loaded {} font(s)", map.names.len());

        Self {
            inner: ArcSwap::from_pointee(map),
            default,
        }
    }

    /// Look up a font by ID. Returns `None` if not found.
    pub fn get(&self, id: &str) -> Option<FontHandle> {
        self.inner.load().get(id)
    }

    /// Resolve a font by ID, falling back to the default if empty or not found.
    pub fn resolve(&self, id: &str) -> FontHandle {
        if id.is_empty() {
            return self.default.clone();
        }
        self.get(id).unwrap_or_else(|| self.default.clone())
    }

    /// List of (id, display_name) pairs for PI dropdowns.
    pub fn list(&self) -> Vec<(String, String)> {
        self.inner.load().names.clone()
    }

    /// Re-scan the user fonts directory and merge with the embedded default.
    #[allow(dead_code)]
    pub fn reload(&self) {
        let mut map = FontMap::new();
        map.insert("default", "Default (Mono)", self.default.clone());
        load_user_fonts(&mut map);
        info!("Reloaded {} font(s)", map.names.len());
        self.inner.store(Arc::new(map));
    }
}

// ── User Font Loading ───────────────────────────────────────────────────────

/// Returns the user fonts directory: `%APPDATA%/{PLUGIN_ID}/fonts/`
fn user_fonts_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(PLUGIN_ID).join("fonts"))
}

/// Load all `.ttf` and `.otf` files from the user fonts directory.
fn load_user_fonts(map: &mut FontMap) {
    let Some(dir) = user_fonts_dir() else {
        return;
    };

    if !dir.is_dir() {
        debug!("User fonts directory does not exist: {}", dir.display());
        return;
    }

    let Ok(entries) = std::fs::read_dir(&dir) else {
        warn!("Failed to read user fonts directory: {}", dir.display());
        return;
    };

    let mut reg = FontRegistry::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("ttf" | "otf") => {}
            _ => continue,
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        let id = stem.to_string();

        match reg.load_file(&id, &path) {
            Ok(handle) => {
                // Use filename stem as both ID and display name
                let display_name = humanize_font_name(&id);
                info!("Loaded user font '{}' from {}", id, path.display());
                map.insert(id, display_name, handle);
            }
            Err(e) => {
                warn!("Failed to load font from {}: {e}", path.display());
            }
        }
    }
}

/// Turn a filename stem like "Inter-Regular" into "Inter Regular".
fn humanize_font_name(stem: &str) -> String {
    stem.replace(['-', '_'], " ")
}
