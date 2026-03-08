use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::debug;

use super::model::*;
use super::translations::Translations;

/// Parse the defaultProfile.xml text into the binding data model.
pub fn parse_default_profile(xml: &str, translations: &Translations) -> Result<ParsedBindings> {
    let doc = roxmltree::Document::parse(xml).context("Failed to parse defaultProfile.xml")?;

    let root = doc.root_element();
    let mut activation_modes = HashMap::new();
    let mut action_maps = Vec::new();

    // Parse activation modes (under <ActivationModes><ActivationMode .../>)
    if let Some(modes_node) = root.children().find(|n| n.has_tag_name("ActivationModes")) {
        for mode_node in modes_node
            .children()
            .filter(|n| n.has_tag_name("ActivationMode"))
        {
            if let Some(mode) = parse_activation_mode(&mode_node) {
                activation_modes.insert(mode.name.clone(), mode);
            }
        }
    }
    debug!("Parsed {} activation modes", activation_modes.len());

    // Parse action maps (under <actionmap name="...">)
    for map_node in root.children().filter(|n| n.has_tag_name("actionmap")) {
        if let Some(map) = parse_action_map(&map_node, translations) {
            action_maps.push(map);
        }
    }

    debug!(
        "Parsed {} action maps with {} total actions",
        action_maps.len(),
        action_maps.iter().map(|m| m.actions.len()).sum::<usize>()
    );

    Ok(ParsedBindings {
        action_maps,
        activation_modes,
    })
}

fn parse_activation_mode(node: &roxmltree::Node) -> Option<ActivationMode> {
    let name = node.attribute("name")?.to_string();
    let on_press_str = node.attribute("onPress").unwrap_or("press");
    let on_press = match on_press_str.to_lowercase().as_str() {
        "press" | "pp_press" => ActivationBehavior::Press,
        "hold" | "pp_hold" => ActivationBehavior::Hold,
        "release" | "pp_release" => ActivationBehavior::Release,
        "doubletap" | "pp_doubletap" => ActivationBehavior::DoubleTap,
        _ => ActivationBehavior::Press,
    };

    let hold_trigger_delay = node
        .attribute("holdTriggerDelay")
        .and_then(|s| s.parse().ok());
    let hold_repeat_delay = node
        .attribute("holdRepeatDelay")
        .and_then(|s| s.parse().ok());

    Some(ActivationMode {
        name,
        on_press,
        hold_trigger_delay,
        hold_repeat_delay,
    })
}

fn parse_action_map(node: &roxmltree::Node, translations: &Translations) -> Option<ActionMap> {
    let name = node.attribute("name")?;
    let name: Arc<str> = Arc::from(name);

    // UICategory groups maps that can be active simultaneously
    let ui_category_key = node.attribute("UICategory").unwrap_or("");
    let ui_category: Arc<str> = Arc::from(ui_category_key);

    // UILabel is the display label for this map; fall back to UICategory, then humanize
    let ui_label_key = node.attribute("UILabel").unwrap_or("");
    let ui_label = if !ui_label_key.is_empty() {
        translations.lookup_or_humanize(ui_label_key)
    } else if !ui_category_key.is_empty() {
        translations.lookup_or_humanize(ui_category_key)
    } else {
        super::translations::humanize_label(&name).into()
    };

    let mut actions = Vec::new();
    for action_node in node.children().filter(|n| n.has_tag_name("action")) {
        if let Some(action) = parse_game_action(&action_node, translations) {
            actions.push(action);
        }
    }

    Some(ActionMap {
        name,
        ui_label,
        ui_category,
        actions,
    })
}

/// Device attribute names and their corresponding device types.
const DEVICE_ATTRS: &[(&str, Device)] = &[
    ("keyboard", Device::Keyboard),
    ("mouse", Device::Mouse),
    ("joystick", Device::Joystick),
    ("gamepad", Device::Gamepad),
];

fn parse_game_action(node: &roxmltree::Node, translations: &Translations) -> Option<GameAction> {
    let name = node.attribute("name")?;
    let name: Arc<str> = Arc::from(name);

    // UI label lookup: try @ui_{name} first, then the UILabel attribute
    let ui_label_key = node.attribute("UILabel").unwrap_or("");
    let ui_label = if !ui_label_key.is_empty() {
        translations.lookup_or_humanize(ui_label_key)
    } else {
        // Try to find a translation for the action name itself
        let auto_key = format!("@ui_{name}");
        translations.lookup_or_humanize(&auto_key)
    };

    let activation_mode = node.attribute("activationMode").map(|s| s.to_string());

    let mut bindings = Vec::new();

    // 1. Parse device attribute bindings (defaultProfile format)
    //    e.g. keyboard="ralt+y", gamepad="shoulderl+a"
    for &(attr, device) in DEVICE_ATTRS {
        if let Some(value) = node.attribute(attr) {
            if let Some(binding) = parse_attribute_binding(value, device, attr) {
                bindings.push(binding);
            }
        }
    }

    // 2. Parse device-specific child overrides (e.g. <gamepad input="..."/>)
    for &(tag, device) in DEVICE_ATTRS {
        for child in node.children().filter(|n| n.has_tag_name(tag)) {
            if let Some(value) = child.attribute("input") {
                if let Some(binding) = parse_attribute_binding(value, device, tag) {
                    bindings.push(binding);
                }
            }
        }
    }

    // 3. Parse <rebind> child elements (user overlay / alternate format)
    for rebind in node.children().filter(|n| n.has_tag_name("rebind")) {
        if let Some(binding) = parse_rebind_binding(&rebind) {
            bindings.push(binding);
        }
    }

    Some(GameAction {
        name,
        ui_label,
        bindings,
        activation_mode,
    })
}

/// Parse a binding from a device attribute value like "ralt+y" or "m".
///
/// The value may contain modifiers separated by `+`. Modifier keys are identified
/// from a known list (lshift, rshift, lctrl, rctrl, lalt, ralt). The remaining
/// non-modifier part is the main key.
fn parse_attribute_binding(value: &str, device: Device, device_prefix: &str) -> Option<Binding> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == " " {
        return None;
    }

    let parts: Vec<&str> = trimmed.split('+').collect();
    let mut modifiers = Vec::new();
    let mut main_key = None;

    for part in &parts {
        if is_modifier_key(part) {
            modifiers.push(part.to_string());
        } else if main_key.is_none() {
            // First non-modifier part is the main key
            main_key = Some(*part);
        } else {
            // Subsequent non-modifier parts are device-specific modifiers
            // (e.g. gamepad "shoulderl" in "dpad_up+shoulderl")
            modifiers.push(part.to_string());
        }
    }

    // If all parts are modifiers, the last one is the main key (standalone modifier press)
    let popped;
    if main_key.is_none() && !modifiers.is_empty() {
        popped = modifiers.pop().unwrap();
        main_key = Some(&popped);
    }

    let key = main_key?;
    let input = format!("{device_prefix}+{key}");

    Some(Binding {
        device,
        input,
        modifiers,
    })
}

/// Parse a binding from a <rebind input="kb1_f"/> element.
fn parse_rebind_binding(node: &roxmltree::Node) -> Option<Binding> {
    let input = node.attribute("input")?;
    if input.is_empty() || input == " " {
        return None;
    }

    let device = detect_device(input);

    Some(Binding {
        device,
        input: input.to_string(),
        modifiers: Vec::new(),
    })
}

pub(crate) fn is_modifier_key(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "lshift" | "rshift" | "lctrl" | "rctrl" | "lalt" | "ralt"
    )
}

/// Detect the input device from the binding string prefix.
fn detect_device(input: &str) -> Device {
    let lower = input.to_lowercase();
    if lower.starts_with("kb") || lower.starts_with("keyboard") {
        Device::Keyboard
    } else if lower.starts_with("mo") || lower.starts_with("mouse") {
        Device::Mouse
    } else if lower.starts_with("gp") || lower.starts_with("gamepad") || lower.starts_with("xi_") {
        Device::Gamepad
    } else if lower.starts_with("js") || lower.starts_with("joystick") {
        Device::Joystick
    } else {
        // Default to keyboard for unrecognized prefixes
        Device::Keyboard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_device_from_input() {
        assert_eq!(detect_device("kb1_f"), Device::Keyboard);
        assert_eq!(detect_device("keyboard+f"), Device::Keyboard);
        assert_eq!(detect_device("mo1_button1"), Device::Mouse);
        assert_eq!(detect_device("mouse+button1"), Device::Mouse);
        assert_eq!(detect_device("gp1_a"), Device::Gamepad);
        assert_eq!(detect_device("xi_a"), Device::Gamepad);
        assert_eq!(detect_device("js1_button1"), Device::Joystick);
        assert_eq!(detect_device("joystick+button1"), Device::Joystick);
    }

    #[test]
    fn parse_rebind_format() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ActionMaps>
  <ActivationModes>
    <ActivationMode name="press" onPress="pp_press"/>
    <ActivationMode name="hold" onPress="pp_hold" holdTriggerDelay="0.5"/>
  </ActivationModes>
  <actionmap name="spaceship_general" UICategory="@ui_CCSpaceFlight">
    <action name="v_power_toggle" UILabel="@ui_CIPowerToggle" activationMode="press">
      <rebind input="kb1_f"/>
    </action>
    <action name="v_strafe_forward" UILabel="@ui_CIStrafeForward">
      <rebind input="kb1_w"/>
      <rebind input="js1_y"/>
    </action>
  </actionmap>
</ActionMaps>"#;

        let translations = Translations::default();
        let result = parse_default_profile(xml, &translations).unwrap();

        assert_eq!(result.activation_modes.len(), 2);
        assert!(result.activation_modes.contains_key("press"));
        assert!(result.activation_modes.contains_key("hold"));

        assert_eq!(result.action_maps.len(), 1);
        let map = &result.action_maps[0];
        assert_eq!(map.name.as_ref(), "spaceship_general");
        assert_eq!(map.actions.len(), 2);

        let power = &map.actions[0];
        assert_eq!(power.name.as_ref(), "v_power_toggle");
        assert_eq!(power.bindings.len(), 1);
        assert_eq!(power.bindings[0].device, Device::Keyboard);

        let strafe = &map.actions[1];
        assert_eq!(strafe.bindings.len(), 2);
        assert_eq!(strafe.bindings[0].device, Device::Keyboard);
        assert_eq!(strafe.bindings[1].device, Device::Joystick);
    }

    #[test]
    fn parse_attribute_format() {
        // Real SC defaultProfile format: bindings as attributes on <action>
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<profile version="1">
  <ActivationModes>
    <ActivationMode name="press" onPress="1"/>
    <ActivationMode name="tap" onPress="0" onHold="0" onRelease="1"/>
  </ActivationModes>
  <actionmap name="seat_general" UILabel="@ui_CGSeatGeneral" UICategory="@ui_CCSeatGeneral">
    <action name="v_emergency_exit" activationMode="tap" keyboard="u+lshift" joystick=" " UILabel="@ui_CIEmergencyExit"/>
    <action name="v_eject" activationMode="press" keyboard="ralt+y" gamepad=" " joystick=" " UILabel="@ui_CIEject"/>
    <action name="v_toggle_mining_mode" activationMode="press" keyboard="m" gamepad=" " joystick=" " UILabel="@ui_CIMiningMode"/>
    <action name="v_unbound_action" activationMode="press" keyboard=" " gamepad=" " UILabel="@ui_Unbound"/>
  </actionmap>
</profile>"#;

        let translations = Translations::default();
        let result = parse_default_profile(xml, &translations).unwrap();

        assert_eq!(result.activation_modes.len(), 2);
        assert_eq!(result.action_maps.len(), 1);

        let map = &result.action_maps[0];
        assert_eq!(map.name.as_ref(), "seat_general");
        assert_eq!(map.actions.len(), 4);

        // v_emergency_exit: keyboard="u+lshift" → key=u, modifier=lshift
        let exit = &map.actions[0];
        assert_eq!(exit.name.as_ref(), "v_emergency_exit");
        let kb = exit
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard)
            .unwrap();
        assert_eq!(kb.input, "keyboard+u");
        assert_eq!(kb.modifiers, vec!["lshift"]);
        // joystick=" " → should not produce a binding
        assert!(exit.bindings.iter().all(|b| b.device != Device::Joystick));

        // v_eject: keyboard="ralt+y" → key=y, modifier=ralt
        let eject = &map.actions[1];
        let kb = eject
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard)
            .unwrap();
        assert_eq!(kb.input, "keyboard+y");
        assert_eq!(kb.modifiers, vec!["ralt"]);

        // v_toggle_mining_mode: keyboard="m" → key=m, no modifiers
        let mining = &map.actions[2];
        let kb = mining
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard)
            .unwrap();
        assert_eq!(kb.input, "keyboard+m");
        assert!(kb.modifiers.is_empty());

        // v_unbound_action: keyboard=" " → no keyboard binding
        let unbound = &map.actions[3];
        assert!(
            unbound
                .bindings
                .iter()
                .all(|b| b.device != Device::Keyboard)
        );
    }

    #[test]
    fn parse_child_device_override() {
        // Some actions have device-specific child elements
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<profile version="1">
  <actionmap name="test" UILabel="@ui_Test">
    <action name="v_self_destruct" keyboard="backspace" UILabel="@ui_Destruct">
      <gamepad activationMode="delayed_press_medium" input="dpad_up+shoulderl"/>
    </action>
  </actionmap>
</profile>"#;

        let translations = Translations::default();
        let result = parse_default_profile(xml, &translations).unwrap();

        let action = &result.action_maps[0].actions[0];
        // keyboard attribute binding
        let kb = action
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard)
            .unwrap();
        assert_eq!(kb.input, "keyboard+backspace");
        // gamepad child element binding
        let gp = action
            .bindings
            .iter()
            .find(|b| b.device == Device::Gamepad)
            .unwrap();
        assert_eq!(gp.input, "gamepad+dpad_up");
    }

    #[test]
    fn parse_real_default_profile() {
        let path = "p4k-extracted/Data/Libs/Config/defaultProfile.xml";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: {path} not found (run extract-p4k first)");
            return;
        }

        let xml = std::fs::read_to_string(path).unwrap();
        let ini_path = "p4k-extracted/Data/Localization/english/global.ini";
        let translations = if std::path::Path::new(ini_path).exists() {
            let ini_bytes = std::fs::read(ini_path).unwrap();
            super::super::translations::parse_global_ini(&ini_bytes)
        } else {
            Translations::default()
        };

        let result = parse_default_profile(&xml, &translations).unwrap();

        // Should have a reasonable number of action maps
        assert!(
            result.action_maps.len() > 10,
            "Expected >10 maps, got {}",
            result.action_maps.len()
        );

        // Should have many actions total
        let total_actions = result.action_count();
        assert!(
            total_actions > 100,
            "Expected >100 actions, got {total_actions}"
        );

        // Should have keyboard bindings (the critical check!)
        let total_kb_bindings: usize = result
            .action_maps
            .iter()
            .flat_map(|m| &m.actions)
            .flat_map(|a| &a.bindings)
            .filter(|b| b.device == Device::Keyboard)
            .count();
        assert!(
            total_kb_bindings > 50,
            "Expected >50 keyboard bindings, got {total_kb_bindings}"
        );

        // Should have activation modes
        assert!(
            result.activation_modes.len() > 5,
            "Expected >5 activation modes, got {}",
            result.activation_modes.len()
        );

        // Spot check: seat_general should exist with v_emergency_exit bound to u+lshift
        let seat = result
            .action_maps
            .iter()
            .find(|m| m.name.as_ref() == "seat_general");
        assert!(seat.is_some(), "seat_general map not found");
        let exit_action = seat
            .unwrap()
            .actions
            .iter()
            .find(|a| a.name.as_ref() == "v_emergency_exit");
        assert!(exit_action.is_some(), "v_emergency_exit action not found");
        let exit_kb = exit_action
            .unwrap()
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard);
        assert!(
            exit_kb.is_some(),
            "v_emergency_exit has no keyboard binding"
        );
        assert_eq!(exit_kb.unwrap().input, "keyboard+u");
        assert!(exit_kb.unwrap().modifiers.contains(&"lshift".to_string()));

        eprintln!(
            "Real data: {} maps, {} actions, {} keyboard bindings, {} activation modes",
            result.action_maps.len(),
            total_actions,
            total_kb_bindings,
            result.activation_modes.len()
        );
    }

    /// End-to-end test: parse real data → executor mapping → autofill pipeline
    #[test]
    fn end_to_end_real_data_pipeline() {
        use std::collections::HashSet;

        use crate::bindings::autofill::{AutofillConfig, generate_bindings, render_xml};
        use crate::bindings::executor;

        let path = "p4k-extracted/Data/Libs/Config/defaultProfile.xml";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: {path} not found (run extract-p4k first)");
            return;
        }

        let xml = std::fs::read_to_string(path).unwrap();
        let ini_path = "p4k-extracted/Data/Localization/english/global.ini";
        let translations = if std::path::Path::new(ini_path).exists() {
            let ini_bytes = std::fs::read(ini_path).unwrap();
            super::super::translations::parse_global_ini(&ini_bytes)
        } else {
            Translations::default()
        };

        let parsed = parse_default_profile(&xml, &translations).unwrap();

        // ── Executor coverage ──
        // Count how many keyboard bindings can be mapped to Key enum
        let all_kb_bindings: Vec<_> = parsed
            .action_maps
            .iter()
            .flat_map(|m| &m.actions)
            .flat_map(|a| &a.bindings)
            .filter(|b| b.device == Device::Keyboard)
            .collect();

        let mut mapped = 0usize;
        let mut unmapped = Vec::new();

        for binding in &all_kb_bindings {
            let dummy_action = super::super::model::GameAction {
                name: "test".into(),
                ui_label: "Test".into(),
                bindings: vec![],
                activation_mode: None,
            };
            if executor::binding_to_combo(binding, &parsed.activation_modes, &dummy_action)
                .is_some()
            {
                mapped += 1;
            } else {
                unmapped.push(binding.input.clone());
            }
        }

        let coverage = if all_kb_bindings.is_empty() {
            0.0
        } else {
            (mapped as f64 / all_kb_bindings.len() as f64) * 100.0
        };

        // Deduplicate unmapped keys for reporting
        let mut unique_unmapped: Vec<String> = unmapped.clone();
        unique_unmapped.sort();
        unique_unmapped.dedup();

        eprintln!(
            "Executor: {mapped}/{} keyboard bindings mapped ({coverage:.1}%)",
            all_kb_bindings.len()
        );
        if !unique_unmapped.is_empty() {
            eprintln!(
                "Unmapped keys ({}):\n  {}",
                unique_unmapped.len(),
                unique_unmapped.join("\n  ")
            );
        }

        // At least 90% should map successfully
        assert!(
            coverage > 90.0,
            "Executor coverage too low: {coverage:.1}% ({mapped}/{})",
            all_kb_bindings.len()
        );

        // ── Autofill pipeline ──
        let config = AutofillConfig::default();
        let generated = generate_bindings(&parsed, &config);

        eprintln!(
            "Autofill: generated {} bindings for unbound actions",
            generated.len()
        );

        // Verify no conflicts within the generated set
        let mut seen_per_group: HashMap<String, HashSet<String>> = HashMap::new();
        for g in &generated {
            let combo = g.combo_key();
            let set = seen_per_group.entry(g.action_map.clone()).or_default();
            assert!(
                set.insert(combo.clone()),
                "Autofill conflict: duplicate {combo} in {}",
                g.action_map
            );
        }

        // Render XML and validate basic structure
        let xml_output = render_xml(&generated, &config.profile_name);
        assert!(xml_output.contains("<?xml"));
        assert!(xml_output.contains("<ActionMaps"));
        assert!(xml_output.contains("</ActionMaps>"));

        eprintln!("End-to-end pipeline: OK");
    }

    /// Test the full `load_bindings` pipeline against a real SC installation.
    #[test]
    fn load_bindings_from_live_installation() {
        let live_path = std::path::Path::new("C:/GAMES/StarCitizen/LIVE");
        let p4k_path = live_path.join("Data.p4k");
        if !p4k_path.exists() {
            eprintln!("Skipping: {} not found", p4k_path.display());
            return;
        }

        let result = crate::bindings::load_bindings(live_path);
        assert!(result.is_ok(), "load_bindings failed: {:?}", result.err());

        let parsed = result.unwrap();
        eprintln!(
            "LIVE install: {} maps, {} actions, {} kb bindings",
            parsed.map_count(),
            parsed.action_count(),
            parsed
                .action_maps
                .iter()
                .flat_map(|m| &m.actions)
                .flat_map(|a| &a.bindings)
                .filter(|b| b.device == Device::Keyboard)
                .count()
        );

        assert!(parsed.map_count() > 10);
        assert!(parsed.action_count() > 100);
    }
}
