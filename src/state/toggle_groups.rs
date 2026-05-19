use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde::Deserialize;
use tracing::{info, warn};

use crate::PLUGIN_ID;

// ── TOML schema ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct ToggleGroupToml {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,

    map: String,
    #[serde(default)]
    toggle: Option<String>,
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    off: Option<String>,

    #[serde(default)]
    label_on: Option<String>,
    #[serde(default)]
    label_off: Option<String>,

    #[serde(default)]
    start_on: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ToggleGroupsFile {
    #[serde(default)]
    group: Vec<ToggleGroupToml>,
}

// ── Public types ────────────────────────────────────────────────────────────────

/// A curated toggle group.
#[derive(Debug, Clone)]
pub struct ToggleGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,

    pub map: String,
    pub toggle: Option<String>,
    pub on: Option<String>,
    pub off: Option<String>,

    pub label_on: Option<String>,
    pub label_off: Option<String>,

    pub start_on: bool,
}

/// Indexed toggle group data for O(1) lookup.
#[derive(Debug, Clone, Default)]
pub struct ToggleGroupsData {
    /// All groups, in source order.
    pub groups: Vec<ToggleGroup>,
    /// `id` → index into `groups`.
    pub index: HashMap<String, usize>,
    /// Legacy `"map.toggle"` → index, for migrating saved settings.
    pub legacy_index: HashMap<String, usize>,
}

impl ToggleGroupsData {
    /// Look up a group by its stable `id`.
    pub fn get(&self, id: &str) -> Option<&ToggleGroup> {
        self.index.get(id).map(|&i| &self.groups[i])
    }

    /// Look up by the legacy `"map.toggle"` compound key. Used to migrate
    /// settings saved under the old schema.
    pub fn get_by_legacy_key(&self, key: &str) -> Option<&ToggleGroup> {
        self.legacy_index.get(key).map(|&i| &self.groups[i])
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
    let mut groups = load_toml_file(&bundled_toml_path()).unwrap_or_default();

    // User overrides: replace by `id`, or append if new.
    if let Some(user_path) = user_toml_path()
        && let Some(user_groups) = load_toml_file(&user_path)
    {
        for ug in user_groups {
            if let Some(existing) = groups.iter_mut().find(|g| g.id == ug.id) {
                *existing = ug;
            } else {
                groups.push(ug);
            }
        }
    }

    build_indexed(groups)
}

fn load_toml_file(path: &std::path::Path) -> Option<Vec<ToggleGroup>> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return Some(vec![]);
    }
    match toml::from_str::<ToggleGroupsFile>(&content) {
        Ok(file) => Some(
            file.group
                .into_iter()
                .map(|g| ToggleGroup {
                    id: g.id,
                    name: g.name,
                    description: g.description,
                    map: g.map,
                    toggle: g.toggle.filter(|s| !s.is_empty()),
                    on: g.on.filter(|s| !s.is_empty()),
                    off: g.off.filter(|s| !s.is_empty()),
                    label_on: g.label_on.filter(|s| !s.is_empty()),
                    label_off: g.label_off.filter(|s| !s.is_empty()),
                    start_on: g.start_on,
                })
                .collect(),
        ),
        Err(e) => {
            warn!("Failed to parse {}: {e}", path.display());
            None
        }
    }
}

fn build_indexed(groups: Vec<ToggleGroup>) -> ToggleGroupsData {
    let mut index = HashMap::with_capacity(groups.len());
    let mut legacy_index = HashMap::with_capacity(groups.len());
    for (i, g) in groups.iter().enumerate() {
        if index.insert(g.id.clone(), i).is_some() {
            warn!("Duplicate toggle group id: {}", g.id);
        }
        if let Some(ref t) = g.toggle {
            legacy_index.insert(format!("{}.{}", g.map, t), i);
        }
    }
    ToggleGroupsData {
        groups,
        index,
        legacy_index,
    }
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
