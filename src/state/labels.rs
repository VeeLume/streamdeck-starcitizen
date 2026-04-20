use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;
use crate::labels::{self, LabelMode};

/// Thread-safe registry of available label modes (built-in + user TOML files).
pub struct LabelsState {
    inner: ArcSwap<Vec<LabelMode>>,
}

impl LabelsState {
    /// Load built-in modes and scan the user labels directory.
    pub fn load() -> Self {
        let mut all = labels::builtins();
        if let Some(dir) = user_labels_dir() {
            all.extend(load_user_modes_from(&dir));
        }
        info!("Loaded {} label mode(s)", all.len());
        Self::from_modes(all)
    }

    /// Build a state from an explicit list of modes (used in tests).
    pub fn from_modes(modes: Vec<LabelMode>) -> Self {
        Self {
            inner: ArcSwap::from_pointee(modes),
        }
    }

    /// Look up a mode by ID.  Returns a clone.
    pub fn get(&self, id: &str) -> Option<LabelMode> {
        self.inner.load().iter().find(|m| m.id == id).cloned()
    }

    /// List of (id, display_name) pairs for PI dropdowns.
    pub fn list(&self) -> Vec<(String, String)> {
        self.inner
            .load()
            .iter()
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect()
    }

    /// Re-scan the user labels directory and merge with built-ins.
    #[allow(dead_code)]
    pub fn reload(&self) {
        let mut all = labels::builtins();
        if let Some(dir) = user_labels_dir() {
            all.extend(load_user_modes_from(&dir));
        }
        info!("Reloaded {} label mode(s)", all.len());
        self.inner.store(Arc::new(all));
    }
}

/// Resolve the effective label mode for a key.
///
/// Priority: per-key `per_key_id` → `global_default_id` → built-in `"smart"`.
pub fn resolve_label_mode(
    per_key_id: &str,
    labels: &LabelsState,
    global_default_id: Option<&str>,
) -> LabelMode {
    // 1. Per-key override
    if !per_key_id.is_empty()
        && let Some(m) = labels.get(per_key_id)
    {
        return m;
    }

    // 2. Global default
    if let Some(id) = global_default_id
        && !id.is_empty()
        && let Some(m) = labels.get(id)
    {
        return m;
    }

    // 3. Built-in default
    labels::mode_smart()
}

// ── User Mode Loading ───────────────────────────────────────────────────────

/// Returns the user labels directory: `%APPDATA%/icu.veelume.starcitizen/labels/`
fn user_labels_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(PLUGIN_ID).join("labels"))
}

/// Load all `.toml` files from the given directory.
fn load_user_modes_from(dir: &Path) -> Vec<LabelMode> {
    if !dir.is_dir() {
        debug!("User labels directory does not exist: {}", dir.display());
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        warn!("Failed to read user labels directory: {}", dir.display());
        return Vec::new();
    };

    let mut modes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        match load_mode_file(&path) {
            Ok(mut mode) => {
                // Use filename stem as ID
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    mode.id = stem.to_string();
                }
                // Use ID as display name if name is empty
                if mode.name.is_empty() {
                    mode.name = mode.id.clone();
                }
                info!(
                    "Loaded user label mode '{}' from {}",
                    mode.id,
                    path.display()
                );
                modes.push(mode);
            }
            Err(e) => {
                warn!("Failed to load label mode from {}: {e}", path.display());
            }
        }
    }

    modes
}

fn load_mode_file(path: &Path) -> anyhow::Result<LabelMode> {
    let contents = std::fs::read_to_string(path)?;
    let mode: LabelMode = toml::from_str(&contents)?;
    Ok(mode)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::LabelStep;
    use std::fs;

    fn mode(id: &str, steps: Vec<LabelStep>) -> LabelMode {
        LabelMode {
            id: id.to_string(),
            name: id.to_string(),
            steps,
        }
    }

    // ── load_user_modes_from ─────────────────────────────────────────────

    #[test]
    fn load_user_modes_missing_dir_returns_empty() {
        let dir = std::env::temp_dir().join("nonexistent_labels_xyz_123");
        assert!(load_user_modes_from(&dir).is_empty());
    }

    #[test]
    fn load_user_modes_skips_non_toml() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("notes.txt"), "ignored").unwrap();
        fs::write(tmp.path().join("data.json"), "{}").unwrap();
        assert!(load_user_modes_from(tmp.path()).is_empty());
    }

    #[test]
    fn load_user_modes_parses_toml_and_uses_filename_stem_as_id() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("shouty.toml"),
            r#"
                name = "Shouty"
                [[steps]]
                type = "uppercase"
            "#,
        )
        .unwrap();

        let modes = load_user_modes_from(tmp.path());
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].id, "shouty");
        assert_eq!(modes[0].name, "Shouty");
        assert!(matches!(modes[0].steps[0], LabelStep::Uppercase));
    }

    #[test]
    fn load_user_modes_uses_id_as_name_when_name_missing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("plain.toml"), "steps = []").unwrap();

        let modes = load_user_modes_from(tmp.path());
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].id, "plain");
        assert_eq!(modes[0].name, "plain");
    }

    #[test]
    fn load_user_modes_skips_invalid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("good.toml"), "steps = []").unwrap();
        fs::write(tmp.path().join("broken.toml"), "this is = not [valid").unwrap();

        let modes = load_user_modes_from(tmp.path());
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].id, "good");
    }

    // ── LabelsState ──────────────────────────────────────────────────────

    #[test]
    fn labels_state_get_and_list() {
        let state = LabelsState::from_modes(vec![
            mode("a", vec![LabelStep::Uppercase]),
            mode("b", vec![LabelStep::Lowercase]),
        ]);

        assert!(state.get("a").is_some());
        assert!(state.get("missing").is_none());
        assert_eq!(state.list().len(), 2);
        assert_eq!(state.list()[0].0, "a");
    }

    // ── resolve_label_mode ───────────────────────────────────────────────

    #[test]
    fn resolve_prefers_per_key_over_global() {
        let state = LabelsState::from_modes(vec![
            mode("per-key", vec![LabelStep::Uppercase]),
            mode("global", vec![LabelStep::Lowercase]),
        ]);

        let resolved = resolve_label_mode("per-key", &state, Some("global"));
        assert_eq!(resolved.id, "per-key");
    }

    #[test]
    fn resolve_falls_back_to_global_when_per_key_empty() {
        let state = LabelsState::from_modes(vec![mode("global", vec![LabelStep::Lowercase])]);

        let resolved = resolve_label_mode("", &state, Some("global"));
        assert_eq!(resolved.id, "global");
    }

    #[test]
    fn resolve_falls_back_to_smart_when_no_overrides() {
        // State has the built-in 'smart' mode
        let state = LabelsState::from_modes(labels::builtins());

        let resolved = resolve_label_mode("", &state, None);
        assert_eq!(resolved.id, "smart");
    }

    #[test]
    fn resolve_falls_back_to_smart_when_per_key_id_unknown() {
        let state = LabelsState::from_modes(labels::builtins());

        let resolved = resolve_label_mode("does-not-exist", &state, None);
        assert_eq!(resolved.id, "smart");
    }

    #[test]
    fn resolve_falls_back_to_smart_when_global_id_unknown() {
        let state = LabelsState::from_modes(labels::builtins());

        let resolved = resolve_label_mode("", &state, Some("does-not-exist"));
        assert_eq!(resolved.id, "smart");
    }

    #[test]
    fn resolve_treats_empty_global_id_as_unset() {
        let state = LabelsState::from_modes(labels::builtins());

        let resolved = resolve_label_mode("", &state, Some(""));
        assert_eq!(resolved.id, "smart");
    }
}
