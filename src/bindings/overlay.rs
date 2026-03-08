use anyhow::{Context, Result};
use tracing::debug;

use super::model::*;
use super::parser::is_modifier_key;

/// A user override from actionmaps.xml.
#[derive(Debug, Clone)]
pub struct UserOverride {
    pub action_map: String,
    pub action_name: String,
    pub bindings: Vec<Binding>,
}

/// Parse the user's actionmaps.xml overlay file.
///
/// Handles the real SC format: `<ActionMaps> > <ActionProfiles> > <actionmap>`
/// as well as the simpler `<ActionMaps> > <actionmap>` used in tests/generators.
pub fn parse_user_overlay(xml: &str) -> Result<Vec<UserOverride>> {
    let doc = roxmltree::Document::parse(xml).context("Failed to parse actionmaps.xml")?;
    let root = doc.root_element();

    let mut overrides = Vec::new();

    // Descend through wrapper elements to find the node containing <actionmap> children.
    // Real SC format: <ActionMaps> > <ActionProfiles> > <actionmap>
    // Generator format: <ActionMaps> > <actionmap>
    let maps_root = root
        .children()
        .find(|n| n.has_tag_name("ActionMaps"))
        .unwrap_or(root);

    // Check for <ActionProfiles> wrapper (present in real SC files)
    let maps_root = maps_root
        .children()
        .find(|n| n.has_tag_name("ActionProfiles"))
        .unwrap_or(maps_root);

    for map_node in maps_root.children().filter(|n| n.has_tag_name("actionmap")) {
        let map_name = match map_node.attribute("name") {
            Some(n) => n.to_string(),
            None => continue,
        };

        for action_node in map_node.children().filter(|n| n.has_tag_name("action")) {
            let action_name = match action_node.attribute("name") {
                Some(n) => n.to_string(),
                None => continue,
            };

            let mut bindings = Vec::new();
            for rebind in action_node.children().filter(|n| n.has_tag_name("rebind")) {
                if let Some(input) = rebind.attribute("input")
                    && !input.is_empty()
                    && input != " "
                    && let Some(binding) = parse_rebind_input(input)
                {
                    bindings.push(binding);
                }
            }

            if !bindings.is_empty() {
                overrides.push(UserOverride {
                    action_map: map_name.clone(),
                    action_name,
                    bindings,
                });
            }
        }
    }

    debug!("Parsed {} user override entries", overrides.len());
    Ok(overrides)
}

/// Parse a rebind input string like `kb1_ralt+rctrl+f2` into a proper Binding.
///
/// Splits on `+` after stripping the device prefix, classifies modifier keys,
/// and normalizes the input to `"keyboard+{main_key}"` format.
fn parse_rebind_input(input: &str) -> Option<Binding> {
    let device = detect_device_from_input(input);
    let device_prefix = match device {
        Device::Keyboard => "keyboard",
        Device::Mouse => "mouse",
        Device::Gamepad => "gamepad",
        Device::Joystick => "joystick",
    };

    let stripped = strip_device_prefix(input);
    let parts: Vec<&str> = stripped.split('+').collect();

    let mut modifiers = Vec::new();
    let mut main_key: Option<String> = None;

    for part in &parts {
        if is_modifier_key(part) {
            modifiers.push(part.to_string());
        } else if main_key.is_none() {
            main_key = Some(part.to_string());
        } else {
            // Additional non-modifier parts treated as device-specific modifiers
            modifiers.push(part.to_string());
        }
    }

    // If all parts are modifiers, the last one is the main key
    if main_key.is_none() && !modifiers.is_empty() {
        main_key = modifiers.pop();
    }

    let key = main_key?;
    let normalized_input = format!("{device_prefix}+{key}");

    Some(Binding {
        device,
        input: normalized_input,
        modifiers,
    })
}

/// Strip the device prefix from a rebind input string.
///
/// `kb1_f` → `f`, `kb1_ralt+rctrl+f2` → `ralt+rctrl+f2`, `mo1_button1` → `button1`
fn strip_device_prefix(input: &str) -> &str {
    let lower = input.to_lowercase();
    // Handle kbN_, moN_, gpN_, jsN_ prefixes
    for prefix in &["kb", "mo", "gp", "js"] {
        if lower.starts_with(prefix)
            && let Some(pos) = input.find('_')
        {
            return &input[pos + 1..];
        }
    }
    // Handle "keyboard+", "mouse+" etc.
    for prefix in &["keyboard+", "mouse+", "gamepad+", "joystick+"] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return rest;
        }
    }
    input
}

/// Apply user overrides on top of the default bindings.
///
/// For each override, find the matching action map and action, then replace its bindings.
pub fn apply_overlay(bindings: &mut ParsedBindings, overrides: &[UserOverride]) {
    for ovr in overrides {
        for map in &mut bindings.action_maps {
            if map.name.as_ref() == ovr.action_map {
                for action in &mut map.actions {
                    if action.name.as_ref() == ovr.action_name {
                        // Merge: add new device bindings, replace existing ones for same device
                        for new_bind in &ovr.bindings {
                            if let Some(existing) = action
                                .bindings
                                .iter_mut()
                                .find(|b| b.device == new_bind.device)
                            {
                                *existing = new_bind.clone();
                            } else {
                                action.bindings.push(new_bind.clone());
                            }
                        }
                    }
                }
            }
        }
    }
}

fn detect_device_from_input(input: &str) -> Device {
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
        Device::Keyboard
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn parse_simple_overlay_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ActionMaps>
  <actionmap name="spaceship_general">
    <action name="v_power_toggle">
      <rebind input="kb1_g"/>
    </action>
  </actionmap>
</ActionMaps>"#;

        let overrides = parse_user_overlay(xml).unwrap();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].action_map, "spaceship_general");
        assert_eq!(overrides[0].action_name, "v_power_toggle");
        assert_eq!(overrides[0].bindings[0].input, "keyboard+g");
        assert!(overrides[0].bindings[0].modifiers.is_empty());
    }

    #[test]
    fn parse_real_sc_overlay_with_action_profiles_wrapper() {
        // Real SC format: <ActionMaps> > <ActionProfiles> > <actionmap>
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ActionMaps>
 <ActionProfiles version="1" optionsVersion="2" rebindVersion="2" profileName="default">
  <deviceoptions name="Mouse">
   <option input="@pause_OptionsMouseSmoothing" saturation="0"/>
  </deviceoptions>
  <options type="keyboard" instance="1" Product="Tastatur"/>
  <modifiers />
  <actionmap name="seat_general">
   <action name="v_enter_remote_turret_1">
    <rebind input="kb1_ralt+rctrl+f2"/>
   </action>
   <action name="v_set_flight_mode">
    <rebind input="kb1_ralt+f2"/>
   </action>
  </actionmap>
  <actionmap name="spaceship_general">
   <action name="v_close_all_doors">
    <rebind input="kb1_lalt+f4"/>
   </action>
  </actionmap>
 </ActionProfiles>
</ActionMaps>"#;

        let overrides = parse_user_overlay(xml).unwrap();
        assert_eq!(overrides.len(), 3);

        // First action: kb1_ralt+rctrl+f2 → modifiers [ralt, rctrl], key f2
        let turret = &overrides[0];
        assert_eq!(turret.action_map, "seat_general");
        assert_eq!(turret.action_name, "v_enter_remote_turret_1");
        assert_eq!(turret.bindings[0].device, Device::Keyboard);
        assert_eq!(turret.bindings[0].input, "keyboard+f2");
        assert_eq!(turret.bindings[0].modifiers, vec!["ralt", "rctrl"]);

        // Second action: kb1_ralt+f2 → modifier [ralt], key f2
        let flight = &overrides[1];
        assert_eq!(flight.action_name, "v_set_flight_mode");
        assert_eq!(flight.bindings[0].input, "keyboard+f2");
        assert_eq!(flight.bindings[0].modifiers, vec!["ralt"]);

        // Third action: kb1_lalt+f4 → modifier [lalt], key f4
        let doors = &overrides[2];
        assert_eq!(doors.action_map, "spaceship_general");
        assert_eq!(doors.bindings[0].input, "keyboard+f4");
        assert_eq!(doors.bindings[0].modifiers, vec!["lalt"]);
    }

    #[test]
    fn parse_rebind_input_simple_key() {
        let b = parse_rebind_input("kb1_g").unwrap();
        assert_eq!(b.device, Device::Keyboard);
        assert_eq!(b.input, "keyboard+g");
        assert!(b.modifiers.is_empty());
    }

    #[test]
    fn parse_rebind_input_with_modifiers() {
        let b = parse_rebind_input("kb1_ralt+rctrl+f2").unwrap();
        assert_eq!(b.device, Device::Keyboard);
        assert_eq!(b.input, "keyboard+f2");
        assert_eq!(b.modifiers, vec!["ralt", "rctrl"]);
    }

    #[test]
    fn parse_rebind_input_numpad() {
        let b = parse_rebind_input("kb1_lalt+rctrl+np_7").unwrap();
        assert_eq!(b.device, Device::Keyboard);
        assert_eq!(b.input, "keyboard+np_7");
        assert_eq!(b.modifiers, vec!["lalt", "rctrl"]);
    }

    #[test]
    fn parse_rebind_input_mouse_device() {
        let b = parse_rebind_input("mo1_rctrl+j").unwrap();
        assert_eq!(b.device, Device::Mouse);
        assert_eq!(b.input, "mouse+j");
        assert_eq!(b.modifiers, vec!["rctrl"]);
    }

    #[test]
    fn apply_overlay_replaces_binding() {
        let mut bindings = ParsedBindings {
            action_maps: vec![ActionMap {
                name: Arc::from("spaceship_general"),
                ui_label: Arc::from("Spaceship"),
                ui_category: Arc::from("@ui_CCSpaceFlight"),
                actions: vec![GameAction {
                    name: Arc::from("v_power_toggle"),
                    ui_label: Arc::from("Toggle Power"),
                    bindings: vec![Binding {
                        device: Device::Keyboard,
                        input: "keyboard+f".to_string(),
                        modifiers: vec![],
                    }],
                    activation_mode: None,
                }],
            }],
            activation_modes: Default::default(),
        };

        let overrides = vec![UserOverride {
            action_map: "spaceship_general".to_string(),
            action_name: "v_power_toggle".to_string(),
            bindings: vec![Binding {
                device: Device::Keyboard,
                input: "keyboard+g".to_string(),
                modifiers: vec!["ralt".to_string()],
            }],
        }];

        apply_overlay(&mut bindings, &overrides);

        let action = &bindings.action_maps[0].actions[0];
        assert_eq!(action.bindings[0].input, "keyboard+g");
        assert_eq!(action.bindings[0].modifiers, vec!["ralt"]);
    }
}
