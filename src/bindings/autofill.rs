use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use tracing::{info, warn};

use super::model::{Binding, Device, ParsedBindings};
use super::overlay::UserOverride;

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
            skip_maps: super::HIDDEN_ACTION_MAPS
                .iter()
                .map(|&s| s.to_string())
                .collect(),
            category_groups: default_category_groups(),
            auto_detect_deny_modifiers: true,
            profile_name: "icu-veelume-starcitizen".to_string(),
        }
    }
}

fn default_candidate_keys() -> Vec<String> {
    let mut keys = Vec::new();
    // F-keys
    for i in 1..=12 {
        keys.push(format!("f{i}"));
    }
    // Numpad digits (SC canonical: np_0..np_9)
    for i in 0..=9 {
        keys.push(format!("np_{i}"));
    }
    // Numpad operators
    keys.extend(
        [
            "np_add",
            "np_subtract",
            "np_multiply",
            "np_divide",
            "np_period",
        ]
        .iter()
        .map(|&s| s.to_string()),
    );
    // Navigation block
    keys.extend(
        ["insert", "delete", "home", "end", "pgup", "pgdn"]
            .iter()
            .map(|&s| s.to_string()),
    );
    // Punctuation / symbol keys
    keys.extend(
        [
            "minus",
            "equals",
            "semicolon",
            "apostrophe",
            "lbracket",
            "rbracket",
            "backslash",
            "comma",
            "period",
            "slash",
        ]
        .iter()
        .map(|&s| s.to_string()),
    );
    // Arrow keys
    keys.extend(
        ["up", "down", "left", "right"]
            .iter()
            .map(|&s| s.to_string()),
    );
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
    set.insert("lctrl+lalt+np_0".into()); // Common system shortcut
    set.insert("lctrl+lalt+delete".into()); // Windows security screen
    set.insert("lctrl+insert".into()); // System copy
    set.insert("lshift+insert".into()); // System paste
    set.insert("lshift+delete".into()); // System cut
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

/// A skipped action that could not be assigned a key combo.
#[derive(Debug, Clone)]
pub struct SkippedAction {
    pub action_map: String,
    pub action_name: String,
}

/// Result of the autofill generation.
#[derive(Debug, Clone)]
pub struct AutofillResult {
    pub generated: Vec<GeneratedBinding>,
    pub skipped: Vec<SkippedAction>,
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
pub fn generate_bindings(bindings: &ParsedBindings, config: &AutofillConfig) -> AutofillResult {
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
    let mut skipped = Vec::new();

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
                warn!(
                    action_map = map.name.as_ref(),
                    action = action.name.as_ref(),
                    "Exhausted all combos — could not assign binding"
                );
                skipped.push(SkippedAction {
                    action_map: map.name.to_string(),
                    action_name: action.name.to_string(),
                });
            }
        }
    }

    if skipped.is_empty() {
        info!(
            "Generated {} autofill bindings (all actions covered)",
            generated.len()
        );
    } else {
        warn!(
            "Generated {} autofill bindings, {} actions skipped (out of combos)",
            generated.len(),
            skipped.len()
        );
    }

    AutofillResult { generated, skipped }
}

/// Render generated bindings as a valid Star Citizen ActionMaps XML.
///
/// `user_overrides` are the user's own customisations from `actionmaps.xml`.
/// Including them in the generated profile prevents SC from resetting user
/// rebinds when the profile is imported.
pub fn render_xml(
    generated: &[GeneratedBinding],
    user_overrides: &[UserOverride],
    profile_name: &str,
) -> String {
    let mut xml = String::new();
    writeln!(xml, "<?xml version=\"1.0\" encoding=\"utf-8\"?>").unwrap();
    writeln!(
        xml,
        "<ActionMaps version=\"1\" optionsVersion=\"2\" rebindVersion=\"2\" profileName=\"{profile_name}\">"
    )
    .unwrap();

    // CustomisationUIHeader is required for SC to recognise the profile
    writeln!(
        xml,
        "  <CustomisationUIHeader label=\"{profile_name}\" description=\"\" image=\"\">"
    )
    .unwrap();
    writeln!(xml, "    <devices>").unwrap();
    writeln!(xml, "      <keyboard instance=\"1\"/>").unwrap();
    writeln!(xml, "      <mouse instance=\"1\"/>").unwrap();
    writeln!(xml, "    </devices>").unwrap();
    writeln!(xml, "  </CustomisationUIHeader>").unwrap();
    writeln!(xml, "  <modifiers/>").unwrap();

    // Collect all actions per map: user overrides first, then generated.
    // Use an ordered map to keep action maps in a stable order.
    let mut by_map: HashMap<String, Vec<ActionEntry>> = HashMap::new();
    // Track which (map, action) pairs came from user overrides so generated
    // bindings don't duplicate them.
    let mut user_actions: HashSet<(String, String)> = HashSet::new();

    for ovr in user_overrides {
        user_actions.insert((ovr.action_map.clone(), ovr.action_name.clone()));
        by_map
            .entry(ovr.action_map.clone())
            .or_default()
            .push(ActionEntry::UserOverride(ovr));
    }

    for b in generated {
        // Skip if this action already has a user override (user takes precedence)
        if user_actions.contains(&(b.action_map.clone(), b.action_name.clone())) {
            continue;
        }
        by_map
            .entry(b.action_map.clone())
            .or_default()
            .push(ActionEntry::Generated(b));
    }

    // Sort map names for deterministic output
    let mut map_names: Vec<&String> = by_map.keys().collect();
    map_names.sort();

    for map_name in map_names {
        let entries = &by_map[map_name];
        writeln!(xml, "  <actionmap name=\"{map_name}\">").unwrap();
        for entry in entries {
            match entry {
                ActionEntry::Generated(b) => {
                    let input = format_input(&b.key, &b.modifiers);
                    writeln!(
                        xml,
                        "    <action name=\"{name}\">\n      <rebind device=\"keyboard\" activationMode=\"press\" input=\"{input}\"/>\n    </action>",
                        name = b.action_name
                    )
                    .unwrap();
                }
                ActionEntry::UserOverride(ovr) => {
                    writeln!(xml, "    <action name=\"{}\">", ovr.action_name).unwrap();
                    for binding in &ovr.bindings {
                        let input = format_user_rebind(binding);
                        writeln!(
                            xml,
                            "      <rebind device=\"{device}\" input=\"{input}\"/>",
                            device = device_name(binding.device),
                        )
                        .unwrap();
                    }
                    // Emit explicit clears so SC doesn't restore defaults
                    for &device in &ovr.cleared_devices {
                        writeln!(
                            xml,
                            "      <rebind device=\"{device}\" input=\"{prefix} \"/>",
                            device = device_name(device),
                            prefix = device_prefix(device),
                        )
                        .unwrap();
                    }
                    writeln!(xml, "    </action>").unwrap();
                }
            }
        }
        writeln!(xml, "  </actionmap>").unwrap();
    }

    writeln!(xml, "</ActionMaps>").unwrap();
    xml
}

enum ActionEntry<'a> {
    Generated(&'a GeneratedBinding),
    UserOverride(&'a UserOverride),
}

/// Format a user binding back into SC's rebind input string (e.g. `kb1_lalt+f4`).
fn format_user_rebind(binding: &Binding) -> String {
    let prefix = device_prefix(binding.device);
    let key = binding.input.split('+').last().unwrap_or(&binding.input);
    if binding.modifiers.is_empty() {
        format!("{prefix}{key}")
    } else {
        // Only the first element gets the device prefix
        let mut parts = Vec::with_capacity(binding.modifiers.len() + 1);
        parts.push(format!("{prefix}{}", binding.modifiers[0]));
        for m in &binding.modifiers[1..] {
            parts.push(m.clone());
        }
        parts.push(key.to_string());
        parts.join("+")
    }
}

fn device_prefix(device: Device) -> &'static str {
    match device {
        Device::Keyboard => "kb1_",
        Device::Mouse => "mo1_",
        Device::Gamepad => "gp1_",
        Device::Joystick => "js1_",
    }
}

fn device_name(device: Device) -> &'static str {
    match device {
        Device::Keyboard => "keyboard",
        Device::Mouse => "mouse",
        Device::Gamepad => "gamepad",
        Device::Joystick => "joystick",
    }
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
    // SC rebind format: only the FIRST element gets the device prefix.
    // e.g. "kb1_rctrl+ralt+f2", NOT "kb1_rctrl+kb1_ralt+kb1_f2"
    if modifiers.is_empty() {
        format!("kb1_{key}")
    } else {
        let mut parts = Vec::with_capacity(modifiers.len() + 1);
        parts.push(format!("kb1_{}", modifiers[0]));
        for m in &modifiers[1..] {
            parts.push(m.clone());
        }
        parts.push(key.to_string());
        parts.join("+")
    }
}

/// Strip device prefix and normalise numpad key names to SC canonical `np_` form.
fn strip_kb_prefix(input: &str) -> String {
    let raw = if let Some(rest) = input.strip_prefix("keyboard+") {
        rest
    } else if input.starts_with("kb")
        && let Some(pos) = input.find('_')
    {
        &input[pos + 1..]
    } else {
        input
    };

    // Normalise numpad_X → np_X to match SC's keybinding_localization.xml
    if let Some(suffix) = raw.strip_prefix("numpad_") {
        format!("np_{suffix}")
    } else {
        raw.to_string()
    }
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
        let result = generate_bindings(&bindings, &config);

        assert!(!result.generated.is_empty());
        assert!(result.skipped.is_empty());
        // v_power_toggle already has kb1_f, should not be generated
        assert!(
            result
                .generated
                .iter()
                .all(|g| g.action_name != "v_power_toggle")
        );
        // v_unbound_action should get a binding
        assert!(
            result
                .generated
                .iter()
                .any(|g| g.action_name == "v_unbound_action")
        );
    }

    #[test]
    fn no_conflicts_within_same_map() {
        let bindings = make_test_bindings();
        let config = AutofillConfig::default();
        let result = generate_bindings(&bindings, &config);

        // Combos within the same action map must be unique
        let mut by_map: HashMap<&str, HashSet<String>> = HashMap::new();
        for g in &result.generated {
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
        let result = generate_bindings(&bindings, &config);
        let xml = render_xml(&result.generated, &[], "test-profile");

        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<ActionMaps"));
        assert!(xml.contains("profileName=\"test-profile\""));
        assert!(xml.contains("<CustomisationUIHeader"));
        assert!(xml.contains("<keyboard instance=\"1\"/>"));
        assert!(xml.contains("<modifiers/>"));
        assert!(xml.contains("device=\"keyboard\""));
        assert!(xml.contains("activationMode=\"press\""));
        assert!(xml.contains("</ActionMaps>"));
    }

    #[test]
    fn modifier_combos_count() {
        let mods = vec!["lctrl".into(), "lshift".into()];
        let combos = generate_modifier_combos(&mods);
        assert_eq!(combos.len(), 4); // 2^2 = 4
        assert!(combos[0].is_empty()); // no modifiers first
    }

    #[test]
    fn strip_kb_prefix_normalises_numpad() {
        assert_eq!(strip_kb_prefix("kb1_numpad_8"), "np_8");
        assert_eq!(strip_kb_prefix("kb1_np_8"), "np_8");
        assert_eq!(strip_kb_prefix("kb1_numpad_add"), "np_add");
        assert_eq!(strip_kb_prefix("kb1_np_add"), "np_add");
        assert_eq!(strip_kb_prefix("kb1_f1"), "f1");
        assert_eq!(strip_kb_prefix("kb1_insert"), "insert");
        assert_eq!(strip_kb_prefix("keyboard+lshift"), "lshift");
    }

    /// Validate that all generated key names are recognised by SC's
    /// `keybinding_localization.xml`. This catches naming mismatches
    /// (e.g. `numpad_8` vs `np_8`) before they ship.
    #[test]
    fn generated_keys_match_sc_localization() {
        let loc_path = "p4k-extracted/Data/Libs/Config/keybinding_localization.xml";
        if !std::path::Path::new(loc_path).exists() {
            eprintln!("Skipping: {loc_path} not found (run extract-p4k first)");
            return;
        }

        // Parse valid key names from keybinding_localization.xml
        let xml = std::fs::read_to_string(loc_path).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let mut valid_keys: HashSet<String> = HashSet::new();
        for device in doc.root_element().children().filter(|n| n.is_element()) {
            let device_name = device.attribute("name").unwrap_or("");
            if device_name != "keyboard" {
                continue;
            }
            for key_node in device.children().filter(|n| n.is_element()) {
                if let Some(name) = key_node.attribute("name") {
                    valid_keys.insert(name.to_string());
                }
            }
        }

        assert!(
            valid_keys.len() > 50,
            "Too few keys parsed from localization: {}",
            valid_keys.len()
        );

        // Generate bindings from real data
        let profile_path = "p4k-extracted/Data/Libs/Config/defaultProfile.xml";
        if !std::path::Path::new(profile_path).exists() {
            eprintln!("Skipping: {profile_path} not found");
            return;
        }
        let profile_xml = std::fs::read_to_string(profile_path).unwrap();
        let translations = crate::bindings::translations::Translations::default();
        let parsed =
            crate::bindings::parser::parse_default_profile(&profile_xml, &translations).unwrap();

        let config = AutofillConfig::default();
        let result = generate_bindings(&parsed, &config);

        assert!(
            !result.generated.is_empty(),
            "No bindings generated — nothing to validate"
        );

        // Validate every generated key and modifier is in the localization file
        let mut unknown = Vec::new();
        for g in &result.generated {
            if !valid_keys.contains(&g.key) {
                unknown.push(format!("key={} (action={})", g.key, g.action_name));
            }
            for m in &g.modifiers {
                if !valid_keys.contains(m) {
                    unknown.push(format!("modifier={m} (action={})", g.action_name));
                }
            }
        }

        if !unknown.is_empty() {
            unknown.sort();
            unknown.dedup();
            panic!(
                "{} generated key name(s) not in SC keybinding_localization.xml:\n  {}",
                unknown.len(),
                unknown.join("\n  ")
            );
        }

        eprintln!(
            "Validated {} generated bindings — all keys match SC localization",
            result.generated.len()
        );
    }

    #[test]
    fn render_xml_preserves_user_overrides() {
        use super::super::model::Binding;
        use super::super::overlay::UserOverride;

        // One generated binding
        let generated = vec![GeneratedBinding {
            action_map: "spaceship_general".into(),
            action_name: "v_autoland".into(),
            key: "f5".into(),
            modifiers: vec![],
        }];

        // Two user overrides: a mouse rebind and a keyboard rebind
        let user_overrides = vec![
            UserOverride {
                action_map: "seat_general".into(),
                action_name: "v_view_look_behind".into(),
                bindings: vec![Binding {
                    device: Device::Mouse,
                    input: "mouse+mouse4".into(),
                    modifiers: vec![],
                }],
                cleared_devices: vec![],
            },
            UserOverride {
                action_map: "seat_general".into(),
                action_name: "v_toggle_missile_mode".into(),
                bindings: vec![Binding {
                    device: Device::Keyboard,
                    input: "keyboard+g".into(),
                    modifiers: vec![],
                }],
                cleared_devices: vec![],
            },
        ];

        let xml = render_xml(&generated, &user_overrides, "test-profile");

        // Generated binding present
        assert!(xml.contains("v_autoland"), "generated action missing");
        assert!(xml.contains("kb1_f5"), "generated key combo missing");

        // User overrides present
        assert!(
            xml.contains("v_view_look_behind"),
            "user mouse rebind missing"
        );
        assert!(xml.contains("mo1_mouse4"), "mouse input missing");
        assert!(
            xml.contains("v_toggle_missile_mode"),
            "user keyboard rebind missing"
        );
        assert!(xml.contains("kb1_g"), "keyboard input missing");

        // Both action maps present
        assert!(xml.contains("spaceship_general"));
        assert!(xml.contains("seat_general"));
    }

    #[test]
    fn render_xml_user_override_takes_precedence_over_generated() {
        use super::super::model::Binding;
        use super::super::overlay::UserOverride;

        // Generated binding for an action
        let generated = vec![GeneratedBinding {
            action_map: "seat_general".into(),
            action_name: "v_toggle_missile_mode".into(),
            key: "f5".into(),
            modifiers: vec!["lctrl".into()],
        }];

        // User also has a binding for the same action
        let user_overrides = vec![UserOverride {
            action_map: "seat_general".into(),
            action_name: "v_toggle_missile_mode".into(),
            bindings: vec![Binding {
                device: Device::Keyboard,
                input: "keyboard+g".into(),
                modifiers: vec![],
            }],
            cleared_devices: vec![],
        }];

        let xml = render_xml(&generated, &user_overrides, "test-profile");

        // User's binding wins
        assert!(xml.contains("kb1_g"), "user binding should win");
        // Generated binding should NOT be present
        assert!(
            !xml.contains("kb1_f5"),
            "generated binding should be suppressed when user override exists"
        );
    }

    #[test]
    fn render_xml_preserves_cleared_bindings() {
        use super::super::overlay::UserOverride;

        let generated = vec![];

        // User cleared a keyboard binding (SC writes "kb1_ ")
        let user_overrides = vec![UserOverride {
            action_map: "seat_general".into(),
            action_name: "v_emergency_exit".into(),
            bindings: vec![],
            cleared_devices: vec![Device::Keyboard],
        }];

        let xml = render_xml(&generated, &user_overrides, "test-profile");

        assert!(xml.contains("v_emergency_exit"), "cleared action missing");
        assert!(
            xml.contains("kb1_ "),
            "clear marker missing — SC needs 'kb1_ ' to suppress the default"
        );
    }

    #[test]
    fn format_input_device_prefix_only_on_first_element() {
        // No modifiers
        assert_eq!(format_input("f5", &[]), "kb1_f5");

        // Single modifier
        assert_eq!(format_input("end", &["rctrl".into()]), "kb1_rctrl+end");

        // Two modifiers — only the first gets kb1_
        assert_eq!(
            format_input("end", &["rctrl".into(), "ralt".into()]),
            "kb1_rctrl+ralt+end"
        );

        // Three modifiers
        assert_eq!(
            format_input("f1", &["lctrl".into(), "lalt".into(), "lshift".into()]),
            "kb1_lctrl+lalt+lshift+f1"
        );
    }
}
