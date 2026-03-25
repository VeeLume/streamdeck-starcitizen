use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;

// ── TOML schema ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct ToggleGroupToml {
    map: String,
    toggle: String,
    on: Option<String>,
    off: Option<String>,
    #[serde(default)]
    exclude: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ToggleGroupsFile {
    #[serde(default)]
    group: Vec<ToggleGroupToml>,
}

// ── Public types ────────────────────────────────────────────────────────────────

/// A resolved toggle group: toggle action name + optional enable/disable siblings.
#[derive(Debug, Clone)]
pub struct ToggleGroup {
    pub map: String,
    pub toggle: String,
    pub on: Option<String>,
    pub off: Option<String>,
}

/// Indexed toggle group data for O(1) lookup.
#[derive(Debug, Clone, Default)]
pub struct ToggleGroupsData {
    /// All groups, ordered by map then toggle name.
    pub groups: Vec<ToggleGroup>,
    /// `"map.toggle"` → index into `groups`.
    pub index: HashMap<String, usize>,
}

impl ToggleGroupsData {
    /// Look up a group by its compound key (`"map.toggle"`).
    pub fn get(&self, key: &str) -> Option<&ToggleGroup> {
        self.index.get(key).map(|&i| &self.groups[i])
    }

    /// Iterate all groups.
    pub fn iter(&self) -> impl Iterator<Item = &ToggleGroup> {
        self.groups.iter()
    }
}

// ── State extension ─────────────────────────────────────────────────────────────

/// Thread-safe store for toggle group definitions (from TOML).
pub struct ToggleGroupsState {
    inner: ArcSwap<ToggleGroupsData>,
}

impl ToggleGroupsState {
    /// Load toggle groups from the bundled TOML and optional user override.
    pub fn load() -> Self {
        let data = load_and_merge();
        info!("Loaded {} toggle group(s)", data.groups.len());
        Self {
            inner: ArcSwap::from_pointee(data),
        }
    }

    /// Get a snapshot of the current toggle groups data.
    pub fn snapshot(&self) -> Arc<ToggleGroupsData> {
        self.inner.load_full()
    }

    /// Reload from disk (e.g., after user edits the override TOML).
    #[allow(dead_code)]
    pub fn reload(&self) {
        let data = load_and_merge();
        info!("Reloaded {} toggle group(s)", data.groups.len());
        self.inner.store(Arc::new(data));
    }
}

// ── Loading & merging ───────────────────────────────────────────────────────────

fn load_and_merge() -> ToggleGroupsData {
    // 1. Load bundled TOML (next to the exe, in the .sdPlugin directory)
    let mut groups = load_bundled();

    // 2. Merge user override TOML from AppData
    let user_groups = load_user_override();
    merge_overrides(&mut groups, user_groups);

    // 3. Build index
    build_indexed(groups)
}

fn load_bundled() -> Vec<ToggleGroup> {
    let path = bundled_toml_path();
    debug!(
        "Looking for bundled toggle-groups.toml at {}",
        path.display()
    );
    load_toml_file(&path).unwrap_or_default()
}

fn load_user_override() -> Vec<(ToggleGroupToml, bool)> {
    let Some(path) = user_toml_path() else {
        return vec![];
    };
    debug!("Looking for user toggle-groups.toml at {}", path.display());
    match std::fs::read_to_string(&path) {
        Ok(content) if !content.trim().is_empty() => {
            match toml::from_str::<ToggleGroupsFile>(&content) {
                Ok(file) => {
                    debug!("Loaded {} user toggle group override(s)", file.group.len());
                    file.group.into_iter().map(|g| (g, true)).collect()
                }
                Err(e) => {
                    warn!("Failed to parse user toggle-groups.toml: {e}");
                    vec![]
                }
            }
        }
        _ => vec![],
    }
}

fn load_toml_file(path: &std::path::Path) -> Option<Vec<ToggleGroup>> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return Some(vec![]);
    }
    match toml::from_str::<ToggleGroupsFile>(&content) {
        Ok(file) => {
            let groups = file
                .group
                .into_iter()
                .filter(|g| !g.exclude)
                .map(|g| ToggleGroup {
                    map: g.map,
                    toggle: g.toggle,
                    on: g.on,
                    off: g.off,
                })
                .collect();
            Some(groups)
        }
        Err(e) => {
            warn!("Failed to parse {}: {e}", path.display());
            None
        }
    }
}

fn merge_overrides(base: &mut Vec<ToggleGroup>, overrides: Vec<(ToggleGroupToml, bool)>) {
    for (ovr, _is_user) in overrides {
        let key = format!("{}.{}", ovr.map, ovr.toggle);

        if ovr.exclude {
            // Remove the group from base
            base.retain(|g| format!("{}.{}", g.map, g.toggle) != key);
            debug!("User excluded toggle group: {key}");
            continue;
        }

        // Replace existing or append
        if let Some(existing) = base
            .iter_mut()
            .find(|g| g.map == ovr.map && g.toggle == ovr.toggle)
        {
            existing.on = ovr.on;
            existing.off = ovr.off;
            debug!("User overrode toggle group: {key}");
        } else {
            base.push(ToggleGroup {
                map: ovr.map,
                toggle: ovr.toggle,
                on: ovr.on,
                off: ovr.off,
            });
            debug!("User added toggle group: {key}");
        }
    }
}

fn build_indexed(groups: Vec<ToggleGroup>) -> ToggleGroupsData {
    let mut index = HashMap::with_capacity(groups.len());
    for (i, g) in groups.iter().enumerate() {
        let key = format!("{}.{}", g.map, g.toggle);
        index.insert(key, i);
    }
    ToggleGroupsData { groups, index }
}

// ── Paths ───────────────────────────────────────────────────────────────────────

/// Bundled TOML path: `<exe_dir>/../toggle-groups.toml`
///
/// The exe lives at `.sdPlugin/bin/plugin.exe`, the TOML at `.sdPlugin/toggle-groups.toml`
/// (one level up from the binary).
fn bundled_toml_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // bin/
        .and_then(|p| p.parent().map(|p| p.join("toggle-groups.toml"))) // .sdPlugin/
        .unwrap_or_else(|| PathBuf::from("toggle-groups.toml"))
}

/// User override TOML: `%APPDATA%/icu.veelume.starcitizen/toggle-groups.toml`
fn user_toml_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join(PLUGIN_ID)
            .join("toggle-groups.toml"),
    )
}
