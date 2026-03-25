use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

use super::autofill::AutofillConfig;

// ── TOML schema ─────────────────────────────────────────────────────────────────

/// Partial overlay of [`AutofillConfig`]. Every field is optional — only fields
/// present in the TOML file override the defaults.
#[derive(Debug, Default, Deserialize)]
struct AutofillConfigToml {
    candidate_keys: Option<Vec<String>>,
    candidate_modifiers: Option<Vec<String>>,
    deny_combos: Option<HashSet<String>>,
    skip_maps: Option<HashSet<String>>,
    category_groups: Option<Vec<Vec<String>>>,
    category_overrides: Option<HashMap<String, String>>,
    auto_detect_deny_modifiers: Option<bool>,
}

impl AutofillConfigToml {
    /// Merge present fields into `base`, leaving absent fields untouched.
    fn merge_into(self, base: &mut AutofillConfig) {
        if let Some(v) = self.candidate_keys {
            base.candidate_keys = v;
        }
        if let Some(v) = self.candidate_modifiers {
            base.candidate_modifiers = v;
        }
        if let Some(v) = self.deny_combos {
            base.deny_combos = v;
        }
        if let Some(v) = self.skip_maps {
            base.skip_maps = v;
        }
        if let Some(v) = self.category_groups {
            base.category_groups = v;
        }
        if let Some(v) = self.category_overrides {
            base.category_overrides = v;
        }
        if let Some(v) = self.auto_detect_deny_modifiers {
            base.auto_detect_deny_modifiers = v;
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────────

/// Path to the generator config file.
pub fn config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(appdata)
        .join("icu.veelume.starcitizen")
        .join("generator.toml")
}

/// Load the generator config, merging the TOML file over defaults.
///
/// - File missing → `Ok(defaults)`
/// - File present, valid → `Ok(merged config)`
/// - File present, parse error → `Err(user-friendly message)`
pub fn load_config() -> std::result::Result<AutofillConfig, String> {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(AutofillConfig::default()),
        Err(e) => return Err(format!("Cannot read config: {e}")),
    };

    if content.trim().is_empty() {
        return Ok(AutofillConfig::default());
    }

    let overlay: AutofillConfigToml =
        toml::from_str(&content).map_err(|e| format_toml_error(&e))?;

    let mut config = AutofillConfig::default();
    overlay.merge_into(&mut config);
    Ok(config)
}

/// Validate the TOML config file. Returns `Ok(())` if the file is absent or
/// valid, `Err(message)` if there is a parse error.
pub fn validate_config() -> std::result::Result<(), String> {
    load_config().map(|_| ())
}

/// Write the default config (with comments) to disk.
pub fn reset_config() -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
    }
    std::fs::write(&path, build_default_toml())
        .with_context(|| format!("Write {}", path.display()))?;
    info!("GeneratorConfig: reset to defaults at {}", path.display());
    Ok(())
}

/// Open the config file in the default text editor. Creates the file with
/// defaults if it does not exist yet.
pub fn open_config() -> Result<()> {
    let path = config_path();
    if !path.exists() {
        reset_config()?;
    }

    let path_str = path.to_string_lossy().to_string();
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &path_str])
        .spawn()
        .with_context(|| format!("Open {path_str}"))?;

    info!("GeneratorConfig: opened {path_str} in editor");
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn format_toml_error(e: &toml::de::Error) -> String {
    if let Some(span) = e.span() {
        format!("TOML error at offset {}: {e}", span.start)
    } else {
        format!("TOML error: {e}")
    }
}

/// Build the default TOML content with explanatory comments.
///
/// All values are sourced from [`AutofillConfig::default()`], so this stays in
/// sync automatically when defaults change.
fn build_default_toml() -> String {
    let config = AutofillConfig::default();
    let mut out = String::new();

    writeln!(
        out,
        "# Generator Settings \u{2014} Star Citizen Stream Deck Plugin"
    )
    .unwrap();
    writeln!(
        out,
        "# Edit any field below to customise autofill behaviour."
    )
    .unwrap();
    writeln!(
        out,
        "# Delete a field (or the whole file) to revert it to its default."
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Keys the generator may assign. Order matters: earlier keys are preferred."
    )
    .unwrap();
    write_string_array(&mut out, "candidate_keys", &config.candidate_keys);
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Modifier keys (up to 6). The generator creates all 2^N combinations."
    )
    .unwrap();
    write_string_array(&mut out, "candidate_modifiers", &config.candidate_modifiers);
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Key combos that must never be assigned (e.g. OS shortcuts)."
    )
    .unwrap();
    let mut deny: Vec<_> = config.deny_combos.iter().cloned().collect();
    deny.sort();
    write_string_array(&mut out, "deny_combos", &deny);
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Action maps to skip entirely (not assigned any bindings)."
    )
    .unwrap();
    let mut skip: Vec<_> = config.skip_maps.iter().cloned().collect();
    skip.sort();
    write_string_array(&mut out, "skip_maps", &skip);
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Groups of UI categories that can be active simultaneously."
    )
    .unwrap();
    writeln!(
        out,
        "# Actions within the same group must not share a key combo."
    )
    .unwrap();
    writeln!(out, "category_groups = [").unwrap();
    for group in &config.category_groups {
        let items: Vec<_> = group.iter().map(|s| format!("\"{s}\"")).collect();
        writeln!(out, "    [{}],", items.join(", ")).unwrap();
    }
    writeln!(out, "]").unwrap();
    writeln!(out).unwrap();

    writeln!(
        out,
        "# Auto-detect modifier keys used as main keys and deny them for that group."
    )
    .unwrap();
    writeln!(
        out,
        "auto_detect_deny_modifiers = {}",
        config.auto_detect_deny_modifiers
    )
    .unwrap();
    writeln!(out).unwrap();

    // TOML table — must be last, or subsequent bare keys get swallowed into it.
    writeln!(out, "# Override the UI category for specific action maps.").unwrap();
    writeln!(
        out,
        "# Fixes maps with missing or incorrect UICategory in SC's defaultProfile.xml."
    )
    .unwrap();
    writeln!(out, "[category_overrides]").unwrap();
    let mut overrides: Vec<_> = config.category_overrides.into_iter().collect();
    overrides.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in overrides {
        writeln!(out, "{k} = \"{v}\"").unwrap();
    }

    out
}

fn write_string_array(out: &mut String, key: &str, items: &[String]) {
    writeln!(out, "{key} = [").unwrap();
    for item in items {
        writeln!(out, "    \"{item}\",").unwrap();
    }
    writeln!(out, "]").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_parses_successfully() {
        let toml_str = build_default_toml();
        let overlay: AutofillConfigToml = toml::from_str(&toml_str).unwrap();
        let mut config = AutofillConfig::default();
        overlay.merge_into(&mut config);

        // Should produce the same values as AutofillConfig::default()
        let defaults = AutofillConfig::default();
        assert_eq!(config.candidate_keys, defaults.candidate_keys);
        assert_eq!(config.candidate_modifiers, defaults.candidate_modifiers);
        assert_eq!(config.deny_combos, defaults.deny_combos);
        assert_eq!(config.skip_maps, defaults.skip_maps);
        assert_eq!(config.category_groups, defaults.category_groups);
        assert_eq!(config.category_overrides, defaults.category_overrides);
        assert_eq!(
            config.auto_detect_deny_modifiers,
            defaults.auto_detect_deny_modifiers
        );
    }

    #[test]
    fn partial_toml_overrides_only_specified_fields() {
        let toml_str = r#"
candidate_keys = ["f1", "f2"]
deny_combos = ["lalt+f4"]
"#;
        let overlay: AutofillConfigToml = toml::from_str(toml_str).unwrap();
        let mut config = AutofillConfig::default();
        let original_modifiers = config.candidate_modifiers.clone();
        overlay.merge_into(&mut config);

        assert_eq!(config.candidate_keys, vec!["f1", "f2"]);
        assert_eq!(config.deny_combos, HashSet::from(["lalt+f4".to_string()]));
        // Unspecified fields keep defaults
        assert_eq!(config.candidate_modifiers, original_modifiers);
    }

    #[test]
    fn empty_toml_returns_defaults() {
        let overlay: AutofillConfigToml = toml::from_str("").unwrap();
        let mut config = AutofillConfig::default();
        let defaults = AutofillConfig::default();
        overlay.merge_into(&mut config);

        assert_eq!(config.candidate_keys, defaults.candidate_keys);
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result = toml::from_str::<AutofillConfigToml>("candidate_keys = [123]");
        assert!(result.is_err());
    }
}
