use std::collections::HashMap;
use std::sync::Arc;

/// The complete parsed binding data from a Star Citizen installation.
#[derive(Debug, Clone, Default)]
pub struct ParsedBindings {
    pub action_maps: Vec<ActionMap>,
    pub activation_modes: HashMap<String, ActivationMode>,
}

impl ParsedBindings {
    pub fn map_count(&self) -> usize {
        self.action_maps.len()
    }

    pub fn action_count(&self) -> usize {
        self.action_maps.iter().map(|m| m.actions.len()).sum()
    }
}

/// A category of game actions (e.g. "spaceship_general", "on_foot").
#[derive(Debug, Clone)]
pub struct ActionMap {
    /// Internal name (e.g. "spaceship_general").
    pub name: Arc<str>,
    /// Translated UI label (e.g. "Spaceship - General").
    pub ui_label: Arc<str>,
    /// The raw localization key (e.g. "@ui_CCSpaceFlight").
    pub ui_category: Arc<str>,
    pub actions: Vec<GameAction>,
}

/// A single bindable game action.
#[derive(Debug, Clone)]
pub struct GameAction {
    /// Internal action name (e.g. "v_power_toggle").
    pub name: Arc<str>,
    /// Translated display label.
    pub ui_label: Arc<str>,
    /// Bindings for this action across input devices.
    pub bindings: Vec<Binding>,
    /// Reference to the activation mode pool (e.g. "press", "hold").
    pub activation_mode: Option<String>,
}

/// A single input binding for an action.
#[derive(Debug, Clone)]
pub struct Binding {
    pub device: Device,
    /// The raw input string (e.g. "keyboard+f", "js1_button3").
    pub input: String,
    /// Modifier keys (e.g. ["lshift", "lctrl"]).
    pub modifiers: Vec<String>,
}

/// Input device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Mouse,
    Gamepad,
    Joystick,
}

/// Defines how a key triggers an action.
#[derive(Debug, Clone)]
pub struct ActivationMode {
    pub name: String,
    pub on_press: ActivationBehavior,
    pub hold_trigger_delay: Option<f64>,
    #[allow(dead_code)] // Parsed from XML, relevant for auto-repeat (not yet implemented)
    pub hold_repeat_delay: Option<f64>,
}

/// The behavior type of an activation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationBehavior {
    Press,
    Hold,
    Release,
    DoubleTap,
}
