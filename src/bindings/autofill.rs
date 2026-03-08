use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use tracing::debug;

use super::model::{Device, ParsedBindings};

// ── Configuration ───────────────────────────────────────────────────────────────

/// Configuration for the autofill generator.
#[derive(Debug, Clone)]
pub struct AutofillConfig {
    /// Candidate keys to assign (default: F1-F12 + numpad 0-9).
    pub candidate_keys: Vec<String>,
    /// Candidate modifier keys (up to 6, generating 2^N combos).
    pub candidate_modifiers: Vec<String>,
    /// Combos that must never be assigned (e.g. "lalt+f4").
    pub deny_combos: HashSet<String>,
    /// Action maps to skip entirely.
    pub skip_maps: HashSet<String>,
    /// Groups of UI categories that can be active simultaneously.
    /// Actions within the same group must not share a key combo.
    pub category_groups: Vec<Vec<String>>,
    /// Auto-detect modifiers used as main keys and deny them for that group.
    pub auto_detect_deny_modifiers: bool,
    /// Profile name for the output XML.
    pub profile_name: String,
}

impl Default for AutofillConfig {
    fn default() -> Self {
        Self {
            candidate_keys: default_candidate_keys(),
            candidate_modifiers: default_candidate_modifiers(),
            deny_combos: default_deny_combos(),
            skip_maps: HashSet::new(),
            category_groups: default_category_groups(),
            auto_detect_deny_modifiers: true,
            profile_name: "icu-veelume-starcitizen".to_string(),
        }
    }
}

fn default_candidate_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for i in 1..=12 {
        keys.push(format!("f{i}"));
    }
    for i in 0..=9 {
        keys.push(format!("numpad_{i}"));
    }
    keys
}

fn default_candidate_modifiers() -> Vec<String> {
    vec![
        "lctrl".into(),
        "rctrl".into(),
        "lalt".into(),
        "ralt".into(),
        "lshift".into(),
        "rshift".into(),
    ]
}

fn default_deny_combos() -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert("lalt+f4".into()); // Windows close
    set.insert("lctrl+lalt+numpad_0".into()); // Common system shortcut
    set
}

fn default_category_groups() -> Vec<Vec<String>> {
    vec![
        vec![
            "@ui_CCSpaceFlight".into(),
            "@ui_CGLightControllerDesc".into(),
            "@ui_CCSeatGeneral".into(),
            "@ui_CG_MFDs".into(),
            "@ui_CGUIGeneral".into(),
            "@ui_CGOpticalTracking".into(),
            "@ui_CGInteraction".into(),
        ],
        vec![
            "@ui_CCVehicle".into(),
            "@ui_CGLightControllerDesc".into(),
            "@ui_CG_MFDs".into(),
            "@ui_CGUIGeneral".into(),
            "@ui_CGOpticalTracking".into(),
            "@ui_CGInteraction".into(),
        ],
        vec![
            "@ui_CCTurrets".into(),
            "@ui_CGUIGeneral".into(),
            "@ui_CGOpticalTracking".into(),
            "@ui_CGInteraction".into(),
        ],
        vec![
            "@ui_CCFPS".into(),
            "@ui_CCEVA".into(),
            "@ui_CGUIGeneral".into(),
            "@ui_CGOpticalTracking".into(),
            "@ui_CGInteraction".into(),
        ],
        vec!["@ui_Map".into(), "@ui_CGUIGeneral".into()],
        vec!["@ui_CGEASpectator".into(), "@ui_CGUIGeneral".into()],
        vec!["@ui_CCCamera".into(), "@ui_CGUIGeneral".into()],
    ]
}

// ── Generated Binding ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GeneratedBinding {
    pub action_map: String,
    pub action_name: String,
    pub key: String,
    pub modifiers: Vec<String>,
}

impl GeneratedBinding {
    pub fn combo_key(&self) -> String {
        let mut parts = self.modifiers.clone();
        parts.push(self.key.clone());
        parts.join("+")
    }
}

// ── Generator ───────────────────────────────────────────────────────────────────

/// Generate conflict-free keyboard bindings for actions that lack them.
pub fn generate_bindings(
    bindings: &ParsedBindings,
    config: &AutofillConfig,
) -> Vec<GeneratedBinding> {
    // Step 1: Build the group index — which group does each map belong to?
    let group_for_category = build_group_index(&config.category_groups);
    let mut next_group_idx = config.category_groups.len();

    // Step 1b: Assign implicit groups for maps whose category isn't in any group.
    // Without this, ungrouped maps have no conflict tracking and every action
    // would get the same first-available combo.
    let mut implicit_groups: HashMap<String, usize> = HashMap::new();

    // Step 2: Collect all existing keyboard bindings as occupied slots, per group
    let mut occupied_per_group: HashMap<usize, HashSet<String>> = HashMap::new();
    // Also track modifier keys used as main keys per group (for auto-deny)
    let mut modifier_as_main_per_group: HashMap<usize, HashSet<String>> = HashMap::new();

    for map in &bindings.action_maps {
        let groups = resolve_groups(
            &map.ui_category,
            &group_for_category,
            &mut implicit_groups,
            &mut next_group_idx,
        );

        for action in &map.actions {
            for binding in &action.bindings {
                if binding.device != Device::Keyboard {
                    continue;
                }
                let key_name = strip_kb_prefix(&binding.input).to_lowercase();

                // Check if this is a modifier key used as a main key
                let is_modifier = matches!(
                    key_name.as_str(),
                    "lctrl" | "rctrl" | "lalt" | "ralt" | "lshift" | "rshift"
                );

                for &group_idx in &groups {
                    // Record the combo as occupied
                    let combo = format_combo(&key_name, &binding.modifiers);
                    occupied_per_group
                        .entry(group_idx)
                        .or_default()
                        .insert(combo);

                    if is_modifier {
                        modifier_as_main_per_group
                            .entry(group_idx)
                            .or_default()
                            .insert(key_name.clone());
                    }
                }
            }
        }
    }

    // Step 3: Build the list of all possible modifier combos (2^N)
    let modifier_combos = generate_modifier_combos(&config.candidate_modifiers);

    // Step 4: For each unbound action, assign a key combo
    let mut generated = Vec::new();

    for map in &bindings.action_maps {
        if config.skip_maps.contains(map.name.as_ref()) {
            continue;
        }

        let groups = resolve_groups(
            &map.ui_category,
            &group_for_category,
            &mut implicit_groups,
            &mut next_group_idx,
        );

        for action in &map.actions {
            // Skip if action already has a keyboard binding
            let has_kb = action.bindings.iter().any(|b| b.device == Device::Keyboard);
            if has_kb {
                continue;
            }

            // Skip axis-only and _long suffix actions
            if action.name.ends_with("_long") {
                continue;
            }

            // Try to find an available combo
            if let Some(binding) = find_available_combo(
                &map.name,
                &action.name,
                &config.candidate_keys,
                &modifier_combos,
                &config.deny_combos,
                &groups,
                &occupied_per_group,
                &modifier_as_main_per_group,
                config.auto_detect_deny_modifiers,
            ) {
                // Mark as occupied in all relevant groups
                let combo_key = binding.combo_key();
                for &group_idx in &groups {
                    occupied_per_group
                        .entry(group_idx)
                        .or_default()
                        .insert(combo_key.clone());
                }
                generated.push(binding);
            } else {
                debug!(
                    action = action.name.as_ref(),
                    "No available combo for action"
                );
            }
        }
    }

    debug!("Generated {} autofill bindings", generated.len());
    generated
}

/// Render generated bindings as a valid Star Citizen ActionMaps XML.
pub fn render_xml(generated: &[GeneratedBinding], profile_name: &str) -> String {
    let mut xml = String::new();
    writeln!(xml, "<?xml version=\"1.0\" encoding=\"utf-8\"?>").unwrap();
    writeln!(
        xml,
        "<ActionMaps version=\"1\" optionsVersion=\"2\" rebindVersion=\"2\" profileName=\"{profile_name}\">"
    )
    .unwrap();

    // Group by action map
    let mut by_map: HashMap<&str, Vec<&GeneratedBinding>> = HashMap::new();
    for b in generated {
        by_map.entry(&b.action_map).or_default().push(b);
    }

    for (map_name, bindings) in &by_map {
        writeln!(xml, "  <actionmap name=\"{map_name}\">").unwrap();
        for b in bindings {
            let input = format_input(&b.key, &b.modifiers);
            writeln!(
                xml,
                "    <action name=\"{}\"><rebind input=\"{input}\"/></action>",
                b.action_name
            )
            .unwrap();
        }
        writeln!(xml, "  </actionmap>").unwrap();
    }

    writeln!(xml, "</ActionMaps>").unwrap();
    xml
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn build_group_index(groups: &[Vec<String>]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, group) in groups.iter().enumerate() {
        for category in group {
            index.entry(category.clone()).or_default().push(i);
        }
    }
    index
}

fn groups_for_map(ui_category: &str, index: &HashMap<String, Vec<usize>>) -> Vec<usize> {
    index.get(ui_category).cloned().unwrap_or_default()
}

/// Resolve group indices for a map, creating an implicit group if the category
/// isn't in any configured group. This ensures conflict tracking always works.
fn resolve_groups(
    ui_category: &str,
    group_for_category: &HashMap<String, Vec<usize>>,
    implicit_groups: &mut HashMap<String, usize>,
    next_group_idx: &mut usize,
) -> Vec<usize> {
    let groups = groups_for_map(ui_category, group_for_category);
    if !groups.is_empty() {
        return groups;
    }
    // Assign an implicit group per unique category
    let idx = *implicit_groups
        .entry(ui_category.to_string())
        .or_insert_with(|| {
            let idx = *next_group_idx;
            *next_group_idx += 1;
            idx
        });
    vec![idx]
}

fn generate_modifier_combos(modifiers: &[String]) -> Vec<Vec<String>> {
    let n = modifiers.len();
    let count = 1usize << n; // 2^n
    let mut combos = Vec::with_capacity(count);

    for mask in 0..count {
        let mut combo = Vec::new();
        for (i, modifier) in modifiers.iter().enumerate() {
            if mask & (1 << i) != 0 {
                combo.push(modifier.clone());
            }
        }
        combos.push(combo);
    }

    // Sort: fewer modifiers first (no-mod combos are preferred)
    combos.sort_by_key(|c| c.len());
    combos
}

fn format_combo(key: &str, modifiers: &[String]) -> String {
    let mut parts: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
    parts.push(key);
    parts.join("+")
}

fn format_input(key: &str, modifiers: &[String]) -> String {
    // SC format: "kb1_key" with modifiers as separate keys
    // For generated bindings, we use the kb1_ prefix
    if modifiers.is_empty() {
        format!("kb1_{key}")
    } else {
        // SC supports modifier notation in rebind input
        let mod_str: Vec<String> = modifiers.iter().map(|m| format!("kb1_{m}")).collect();
        format!("{}+kb1_{key}", mod_str.join("+"))
    }
}

fn strip_kb_prefix(input: &str) -> &str {
    if let Some(rest) = input.strip_prefix("keyboard+") {
        return rest;
    }
    if input.starts_with("kb")
        && let Some(pos) = input.find('_')
    {
        return &input[pos + 1..];
    }
    input
}

#[allow(clippy::too_many_arguments)]
fn find_available_combo(
    map_name: &str,
    action_name: &str,
    candidate_keys: &[String],
    modifier_combos: &[Vec<String>],
    deny_combos: &HashSet<String>,
    groups: &[usize],
    occupied: &HashMap<usize, HashSet<String>>,
    modifier_as_main: &HashMap<usize, HashSet<String>>,
    auto_deny: bool,
) -> Option<GeneratedBinding> {
    for key in candidate_keys {
        for mods in modifier_combos {
            let combo_key = format_combo(key, mods);

            // Check deny list
            if deny_combos.contains(&combo_key) {
                continue;
            }

            // Check auto-deny: if a modifier in this combo is used as a main key
            // in any group this action belongs to, skip it
            if auto_deny && !mods.is_empty() {
                let denied = groups.iter().any(|&g| {
                    if let Some(main_mods) = modifier_as_main.get(&g) {
                        mods.iter().any(|m| main_mods.contains(m))
                    } else {
                        false
                    }
                });
                if denied {
                    continue;
                }
            }

            // Check if occupied in any of our groups
            let is_occupied = groups
                .iter()
                .any(|&g| occupied.get(&g).is_some_and(|set| set.contains(&combo_key)));
            if is_occupied {
                continue;
            }

            return Some(GeneratedBinding {
                action_map: map_name.to_string(),
                action_name: action_name.to_string(),
                key: key.clone(),
                modifiers: mods.clone(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::bindings::model::*;

    fn make_test_bindings() -> ParsedBindings {
        ParsedBindings {
            action_maps: vec![
                ActionMap {
                    name: Arc::from("spaceship_general"),
                    ui_label: Arc::from("Spaceship"),
                    ui_category: Arc::from("@ui_CCSpaceFlight"),
                    actions: vec![
                        GameAction {
                            name: Arc::from("v_power_toggle"),
                            ui_label: Arc::from("Power"),
                            bindings: vec![Binding {
                                device: Device::Keyboard,
                                input: "kb1_f".to_string(),
                                modifiers: vec![],
                            }],
                            activation_mode: None,
                        },
                        GameAction {
                            name: Arc::from("v_unbound_action"),
                            ui_label: Arc::from("Unbound"),
                            bindings: vec![], // No bindings
                            activation_mode: None,
                        },
                    ],
                },
                ActionMap {
                    name: Arc::from("on_foot"),
                    ui_label: Arc::from("On Foot"),
                    ui_category: Arc::from("@ui_CCFPS"),
                    actions: vec![GameAction {
                        name: Arc::from("fps_unbound"),
                        ui_label: Arc::from("FPS Unbound"),
                        bindings: vec![],
                        activation_mode: None,
                    }],
                },
            ],
            activation_modes: HashMap::new(),
        }
    }

    #[test]
    fn generates_bindings_for_unbound_actions() {
        let bindings = make_test_bindings();
        let config = AutofillConfig::default();
        let generated = generate_bindings(&bindings, &config);

        assert!(!generated.is_empty());
        // v_power_toggle already has kb1_f, should not be generated
        assert!(generated.iter().all(|g| g.action_name != "v_power_toggle"));
        // v_unbound_action should get a binding
        assert!(
            generated
                .iter()
                .any(|g| g.action_name == "v_unbound_action")
        );
    }

    #[test]
    fn no_conflicts_within_same_map() {
        let bindings = make_test_bindings();
        let config = AutofillConfig::default();
        let generated = generate_bindings(&bindings, &config);

        // Combos within the same action map must be unique
        let mut by_map: HashMap<&str, HashSet<String>> = HashMap::new();
        for g in &generated {
            let combo = g.combo_key();
            let set = by_map.entry(&g.action_map).or_default();
            assert!(
                set.insert(combo.clone()),
                "Duplicate combo {combo} in map {}",
                g.action_map
            );
        }
    }

    #[test]
    fn renders_valid_xml() {
        let bindings = make_test_bindings();
        let config = AutofillConfig::default();
        let generated = generate_bindings(&bindings, &config);
        let xml = render_xml(&generated, "test-profile");

        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<ActionMaps"));
        assert!(xml.contains("profileName=\"test-profile\""));
        assert!(xml.contains("</ActionMaps>"));
    }

    #[test]
    fn modifier_combos_count() {
        let mods = vec!["lctrl".into(), "lshift".into()];
        let combos = generate_modifier_combos(&mods);
        assert_eq!(combos.len(), 4); // 2^2 = 4
        assert!(combos[0].is_empty()); // no modifiers first
    }
}
