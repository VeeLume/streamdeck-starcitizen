use std::collections::HashMap;

use streamdeck_lib::input::{Action, InputCombo, Key, Modifiers, MouseButton, ScrollDirection};
use tracing::{debug, warn};

use super::model::{ActivationBehavior, ActivationMode, Binding, Device, GameAction};

/// Convert a game action's binding into an `InputCombo` ready for execution.
///
/// Supports keyboard keys, mouse buttons, and scroll wheel. SC sometimes stores
/// mouse inputs (mouse1, mwheel_down, etc.) in the `keyboard` attribute, so we
/// accept `Device::Keyboard` bindings and map the key name to the correct input type.
///
/// Returns `None` if the binding's device is unsupported (joystick, gamepad) or the
/// key name can't be mapped.
pub fn binding_to_combo(
    binding: &Binding,
    activation_modes: &HashMap<String, ActivationMode>,
    action: &GameAction,
) -> Option<InputCombo> {
    // Only keyboard and mouse bindings can be simulated
    if !matches!(binding.device, Device::Keyboard | Device::Mouse) {
        return None;
    }

    let key_name = strip_device_prefix(&binding.input);
    let input_action = sc_name_to_action(key_name)?;

    let mut combo = match input_action {
        Action::Key(key) => InputCombo::key(key),
        Action::Mouse(button) => InputCombo::mouse(button),
        Action::Scroll(direction, amount) => InputCombo::scroll(direction, amount),
    };

    // Map modifier keys and apply them to the combo
    let mut mods = Modifiers::empty();
    for modifier in &binding.modifiers {
        let mod_name = strip_device_prefix(modifier);
        if let Some(m) = sc_name_to_modifier(mod_name) {
            mods |= m;
        } else {
            warn!(modifier = mod_name, "Unknown SC modifier name");
        }
    }
    if !mods.is_empty() {
        combo = combo.also(mods);
    }

    // Apply hold duration from the activation mode if it's a "hold" type
    if let Some(mode_name) = &action.activation_mode
        && let Some(mode) = activation_modes.get(mode_name)
        && mode.on_press == ActivationBehavior::Hold
    {
        let delay_ms = mode
            .hold_trigger_delay
            .map(|d| (d * 1000.0) as u64)
            .unwrap_or(300);
        // Add a small buffer to ensure the hold registers
        combo = combo.held_ms(delay_ms + 50);
    }

    debug!(input = %binding.input, "Mapped SC binding to input");
    Some(combo)
}

/// Strip the device prefix from a Star Citizen input string.
///
/// Examples: `"kb1_f"` → `"f"`, `"keyboard+lshift"` → `"lshift"`, `"kb1_numpad_5"` → `"numpad_5"`
/// Also handles mouse prefixes: `"mouse+button1"` → `"button1"`, `"mo1_button1"` → `"button1"`
fn strip_device_prefix(input: &str) -> &str {
    // Handle "device+key" formats
    for prefix in &["keyboard+", "mouse+"] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return rest;
        }
    }
    // Handle "kbN_key" or "moN_key" format (e.g. kb1_f, mo1_button1)
    if (input.starts_with("kb") || input.starts_with("mo"))
        && let Some(pos) = input.find('_')
    {
        return &input[pos + 1..];
    }
    input
}

/// Map a Star Citizen key name to an `Action` (keyboard key, mouse button, or scroll).
///
/// SC sometimes stores mouse inputs in the `keyboard` attribute (e.g. `keyboard="mouse1"`),
/// so this function tries all input types regardless of the source attribute.
fn sc_name_to_action(name: &str) -> Option<Action> {
    // Try mouse/scroll first (more specific names, avoids false keyboard matches)
    if let Some(btn) = sc_name_to_mouse_button(name) {
        return Some(Action::Mouse(btn));
    }
    if let Some(dir) = sc_name_to_scroll(name) {
        return Some(Action::Scroll(dir, 1));
    }
    sc_key_to_key(name).map(Action::Key)
}

/// Map a Star Citizen key name (after prefix stripping) to a `Key` enum variant.
///
/// Key variant names follow W3C UIEvents KeyboardEvent.code convention.
fn sc_key_to_key(name: &str) -> Option<Key> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        // Letters
        "a" => Some(Key::KeyA),
        "b" => Some(Key::KeyB),
        "c" => Some(Key::KeyC),
        "d" => Some(Key::KeyD),
        "e" => Some(Key::KeyE),
        "f" => Some(Key::KeyF),
        "g" => Some(Key::KeyG),
        "h" => Some(Key::KeyH),
        "i" => Some(Key::KeyI),
        "j" => Some(Key::KeyJ),
        "k" => Some(Key::KeyK),
        "l" => Some(Key::KeyL),
        "m" => Some(Key::KeyM),
        "n" => Some(Key::KeyN),
        "o" => Some(Key::KeyO),
        "p" => Some(Key::KeyP),
        "q" => Some(Key::KeyQ),
        "r" => Some(Key::KeyR),
        "s" => Some(Key::KeyS),
        "t" => Some(Key::KeyT),
        "u" => Some(Key::KeyU),
        "v" => Some(Key::KeyV),
        "w" => Some(Key::KeyW),
        "x" => Some(Key::KeyX),
        "y" => Some(Key::KeyY),
        "z" => Some(Key::KeyZ),

        // Number row
        "0" => Some(Key::Digit0),
        "1" => Some(Key::Digit1),
        "2" => Some(Key::Digit2),
        "3" => Some(Key::Digit3),
        "4" => Some(Key::Digit4),
        "5" => Some(Key::Digit5),
        "6" => Some(Key::Digit6),
        "7" => Some(Key::Digit7),
        "8" => Some(Key::Digit8),
        "9" => Some(Key::Digit9),

        // Function keys
        "f1" => Some(Key::F1),
        "f2" => Some(Key::F2),
        "f3" => Some(Key::F3),
        "f4" => Some(Key::F4),
        "f5" => Some(Key::F5),
        "f6" => Some(Key::F6),
        "f7" => Some(Key::F7),
        "f8" => Some(Key::F8),
        "f9" => Some(Key::F9),
        "f10" => Some(Key::F10),
        "f11" => Some(Key::F11),
        "f12" => Some(Key::F12),

        // Navigation / arrows
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pgup" | "pageup" => Some(Key::PageUp),
        "pgdn" | "pagedown" => Some(Key::PageDown),

        // Editing
        "backspace" => Some(Key::Backspace),
        "delete" | "del" => Some(Key::Delete),
        "insert" | "ins" => Some(Key::Insert),
        "enter" | "return" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "space" => Some(Key::Space),
        "escape" | "esc" => Some(Key::Escape),

        // Punctuation & symbols (W3C names)
        "minus" => Some(Key::Minus),
        "equals" | "equal" => Some(Key::Equal),
        "lbracket" | "[" => Some(Key::BracketLeft),
        "rbracket" | "]" => Some(Key::BracketRight),
        "backslash" => Some(Key::Backslash),
        "semicolon" => Some(Key::Semicolon),
        "apostrophe" | "quote" => Some(Key::Quote),
        "grave" | "tilde" => Some(Key::Backquote),
        "comma" => Some(Key::Comma),
        "period" | "dot" => Some(Key::Period),
        "slash" => Some(Key::Slash),

        // Numpad
        "numpad_0" | "np_0" => Some(Key::Numpad0),
        "numpad_1" | "np_1" => Some(Key::Numpad1),
        "numpad_2" | "np_2" => Some(Key::Numpad2),
        "numpad_3" | "np_3" => Some(Key::Numpad3),
        "numpad_4" | "np_4" => Some(Key::Numpad4),
        "numpad_5" | "np_5" => Some(Key::Numpad5),
        "numpad_6" | "np_6" => Some(Key::Numpad6),
        "numpad_7" | "np_7" => Some(Key::Numpad7),
        "numpad_8" | "np_8" => Some(Key::Numpad8),
        "numpad_9" | "np_9" => Some(Key::Numpad9),
        "numpad_add" | "np_add" => Some(Key::NumpadAdd),
        "numpad_subtract" | "np_subtract" => Some(Key::NumpadSubtract),
        "numpad_multiply" | "np_multiply" => Some(Key::NumpadMultiply),
        "numpad_divide" | "np_divide" => Some(Key::NumpadDivide),
        "numpad_enter" | "np_enter" => Some(Key::NumpadEnter),
        "numpad_decimal" | "np_period" => Some(Key::NumpadDecimal),
        "numlock" => Some(Key::NumLock),

        // Modifiers (as standalone keys)
        "lctrl" => Some(Key::ControlLeft),
        "rctrl" => Some(Key::ControlRight),
        "lalt" => Some(Key::AltLeft),
        "ralt" => Some(Key::AltRight),
        "lshift" => Some(Key::ShiftLeft),
        "rshift" => Some(Key::ShiftRight),

        // Lock / system keys
        "capslock" => Some(Key::CapsLock),
        "scrolllock" => Some(Key::ScrollLock),
        "printscreen" | "print" => Some(Key::PrintScreen),
        "pause" => Some(Key::Pause),

        _ => {
            warn!(key = name, "Unknown SC key name");
            None
        }
    }
}

/// Map a Star Citizen modifier key name to a `Modifiers` flag.
fn sc_name_to_modifier(name: &str) -> Option<Modifiers> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "lctrl" => Some(Modifiers::CONTROL_LEFT),
        "rctrl" => Some(Modifiers::CONTROL_RIGHT),
        "lalt" => Some(Modifiers::ALT_LEFT),
        "ralt" => Some(Modifiers::ALT_RIGHT),
        "lshift" => Some(Modifiers::SHIFT_LEFT),
        "rshift" => Some(Modifiers::SHIFT_RIGHT),
        _ => None,
    }
}

/// Map a Star Citizen mouse button name to `MouseButton`.
///
/// SC uses `mouse1`/`mouse2`/`mouse3` in bindings. Also handles `button1` etc.
/// for bindings under the `mouse` device attribute.
fn sc_name_to_mouse_button(name: &str) -> Option<MouseButton> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "mouse1" | "button1" => Some(MouseButton::Left),
        "mouse2" | "button2" => Some(MouseButton::Right),
        "mouse3" | "button3" => Some(MouseButton::Middle),
        "mouse4" | "button4" => Some(MouseButton::X1),
        "mouse5" | "button5" => Some(MouseButton::X2),
        _ => None,
    }
}

/// Map a Star Citizen scroll name to `ScrollDirection`.
fn sc_name_to_scroll(name: &str) -> Option<ScrollDirection> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "mwheel_up" => Some(ScrollDirection::Up),
        "mwheel_down" => Some(ScrollDirection::Down),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_kb_prefix() {
        assert_eq!(strip_device_prefix("kb1_f"), "f");
        assert_eq!(strip_device_prefix("kb2_space"), "space");
        assert_eq!(strip_device_prefix("keyboard+lshift"), "lshift");
        assert_eq!(strip_device_prefix("kb1_numpad_5"), "numpad_5");
        // Mouse prefixes
        assert_eq!(strip_device_prefix("mouse+button1"), "button1");
        assert_eq!(strip_device_prefix("mo1_button1"), "button1");
    }

    #[test]
    fn map_common_keys() {
        assert_eq!(sc_key_to_key("f"), Some(Key::KeyF));
        assert_eq!(sc_key_to_key("space"), Some(Key::Space));
        assert_eq!(sc_key_to_key("f1"), Some(Key::F1));
        assert_eq!(sc_key_to_key("numpad_0"), Some(Key::Numpad0));
        assert_eq!(sc_key_to_key("lshift"), Some(Key::ShiftLeft));
        assert_eq!(sc_key_to_key("escape"), Some(Key::Escape));
        assert_eq!(sc_key_to_key("enter"), Some(Key::Enter));
    }

    #[test]
    fn map_mouse_buttons() {
        assert_eq!(
            sc_name_to_action("mouse1"),
            Some(Action::Mouse(MouseButton::Left))
        );
        assert_eq!(
            sc_name_to_action("mouse2"),
            Some(Action::Mouse(MouseButton::Right))
        );
        assert_eq!(
            sc_name_to_action("mouse3"),
            Some(Action::Mouse(MouseButton::Middle))
        );
        // Also works with stripped "button1" form
        assert_eq!(
            sc_name_to_action("button1"),
            Some(Action::Mouse(MouseButton::Left))
        );
    }

    #[test]
    fn map_scroll_wheel() {
        assert_eq!(
            sc_name_to_action("mwheel_up"),
            Some(Action::Scroll(ScrollDirection::Up, 1))
        );
        assert_eq!(
            sc_name_to_action("mwheel_down"),
            Some(Action::Scroll(ScrollDirection::Down, 1))
        );
    }

    #[test]
    fn binding_to_combo_basic() {
        let binding = Binding {
            device: Device::Keyboard,
            input: "kb1_f".to_string(),
            modifiers: vec![],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: None,
        };
        let modes = HashMap::new();

        let combo = binding_to_combo(&binding, &modes, &action);
        assert!(combo.is_some());
    }

    #[test]
    fn binding_to_combo_mouse_in_keyboard_attr() {
        // SC stores mouse buttons in the keyboard attribute sometimes
        let binding = Binding {
            device: Device::Keyboard,
            input: "keyboard+mouse1".to_string(),
            modifiers: vec![],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: None,
        };
        let modes = HashMap::new();

        let combo = binding_to_combo(&binding, &modes, &action).unwrap();
        assert_eq!(combo.action, Action::Mouse(MouseButton::Left));
    }

    #[test]
    fn non_simulatable_binding_returns_none() {
        let binding = Binding {
            device: Device::Joystick,
            input: "js1_button1".to_string(),
            modifiers: vec![],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: None,
        };
        let modes = HashMap::new();

        assert!(binding_to_combo(&binding, &modes, &action).is_none());
    }

    #[test]
    fn binding_to_combo_includes_modifiers() {
        // LAlt+F6 — should produce F6 with ALT_LEFT modifier
        let binding = Binding {
            device: Device::Keyboard,
            input: "kb1_f6".to_string(),
            modifiers: vec!["kb1_lalt".to_string()],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: None,
        };
        let modes = HashMap::new();

        let combo = binding_to_combo(&binding, &modes, &action).unwrap();
        assert_eq!(combo.action, Action::Key(Key::F6));
        assert!(combo.modifiers.contains(Modifiers::ALT_LEFT));
    }

    #[test]
    fn binding_to_combo_multiple_modifiers() {
        // RCtrl+RAlt+End
        let binding = Binding {
            device: Device::Keyboard,
            input: "kb1_end".to_string(),
            modifiers: vec!["kb1_rctrl".to_string(), "kb1_ralt".to_string()],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: None,
        };
        let modes = HashMap::new();

        let combo = binding_to_combo(&binding, &modes, &action).unwrap();
        assert_eq!(combo.action, Action::Key(Key::End));
        assert!(combo.modifiers.contains(Modifiers::CONTROL_RIGHT));
        assert!(combo.modifiers.contains(Modifiers::ALT_RIGHT));
    }

    #[test]
    fn hold_mode_adds_duration() {
        let binding = Binding {
            device: Device::Keyboard,
            input: "kb1_f".to_string(),
            modifiers: vec![],
        };
        let action = GameAction {
            name: "test".into(),
            ui_label: "Test".into(),
            bindings: vec![],
            activation_mode: Some("hold".to_string()),
        };
        let mut modes = HashMap::new();
        modes.insert(
            "hold".to_string(),
            ActivationMode {
                name: "hold".to_string(),
                on_press: ActivationBehavior::Hold,
                hold_trigger_delay: Some(0.5),
                hold_repeat_delay: None,
            },
        );

        let combo = binding_to_combo(&binding, &modes, &action).unwrap();
        assert!(combo.hold.is_some());
    }
}
