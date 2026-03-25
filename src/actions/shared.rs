//! Shared helpers used by multiple actions (execute_action, toggle_action).

use std::sync::Arc;

use streamdeck_lib::input::InputCombo;
use streamdeck_lib::prelude::*;

use crate::bindings::HIDDEN_ACTION_MAPS;
use crate::bindings::executor::binding_to_combo;
use crate::bindings::model::{Binding, Device};
use crate::state::bindings::BindingsState;
use crate::state::fonts::FontsState;
use crate::state::icon_folder::IconFolderState;
use crate::state::styles::StylesState;

// ── Binding resolution ──────────────────────────────────────────────────────────

/// Look up a game action's keyboard binding and convert it to an `InputCombo`.
pub fn resolve_combo(cx: &Context, map_name: &str, action_name: &str) -> Option<InputCombo> {
    let bindings_state = cx.try_ext::<BindingsState>()?;
    bindings_state.with_bindings(|bindings| {
        let action = bindings
            .action_maps
            .iter()
            .filter(|m| m.name.as_ref() == map_name)
            .flat_map(|m| &m.actions)
            .find(|a| a.name.as_ref() == action_name)?;

        let kb_binding = action
            .bindings
            .iter()
            .find(|b| b.device == Device::Keyboard)?;

        binding_to_combo(kb_binding, &bindings.activation_modes, action)
    })?
}

/// Describe an action for PI display: `(label, keybinding_string)`.
pub fn describe_action(cx: &Context, map_name: &str, action_name: &str) -> (String, String) {
    if action_name.is_empty() {
        return (String::new(), String::new());
    }
    let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
        return (action_name.to_string(), String::new());
    };
    let result = bindings_state.with_bindings(|bindings| {
        let action = bindings
            .action_maps
            .iter()
            .filter(|m| m.name.as_ref() == map_name)
            .flat_map(|m| &m.actions)
            .find(|a| a.name.as_ref() == action_name);

        match action {
            Some(a) => {
                let label = a.ui_label.to_string();
                let bind = a
                    .bindings
                    .iter()
                    .find(|b| matches!(b.device, Device::Keyboard | Device::Mouse))
                    .map(format_binding_display)
                    .unwrap_or_else(|| "(no keyboard bind)".to_string());
                (label, bind)
            }
            None => (action_name.to_string(), String::new()),
        }
    });
    result.unwrap_or_else(|| (action_name.to_string(), String::new()))
}

// ── Datasource helpers ──────────────────────────────────────────────────────────

/// Build the category list for a PI dropdown (all non-hidden action maps).
pub fn get_category_items(cx: &Context) -> Vec<DataSourceResultItem> {
    let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
        return vec![];
    };
    let result = bindings_state.with_bindings(|bindings| {
        let mut seen = std::collections::HashSet::new();
        bindings
            .action_maps
            .iter()
            .filter(|m| !HIDDEN_ACTION_MAPS.contains(&m.name.as_ref()))
            .filter(|m| seen.insert(m.ui_label.as_ref().to_string()))
            .map(|m| {
                DataSourceResultItem::Item(DataSourceItem {
                    disabled: None,
                    label: Some(m.ui_label.to_string()),
                    value: m.name.to_string(),
                })
            })
            .collect::<Vec<_>>()
    });
    result.unwrap_or_default()
}

/// Build the action list for a PI dropdown (all actions in a map, minus `_long`).
pub fn get_action_items(cx: &Context, map_name: &str) -> Vec<DataSourceResultItem> {
    let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
        return vec![];
    };
    let result = bindings_state.with_bindings(|bindings| {
        bindings
            .action_maps
            .iter()
            .filter(|m| m.name.as_ref() == map_name)
            .flat_map(|m| &m.actions)
            .filter(|a| !a.name.ends_with("_long"))
            .map(|a| {
                DataSourceResultItem::Item(DataSourceItem {
                    disabled: None,
                    label: Some(a.ui_label.to_string()),
                    value: a.name.to_string(),
                })
            })
            .collect::<Vec<_>>()
    });
    result.unwrap_or_default()
}

/// Push action items for a datasource event to the PI.
///
/// sdpi-components intercepts `sendToPropertyInspector` messages with
/// `{"event": "<datasource>", "items": [...]}` and updates the corresponding
/// `<sdpi-select datasource="...">` dropdown.
pub fn push_action_items(cx: &Context, ctx_id: &str, event: &str, map_name: &str) {
    let items = get_action_items(cx, map_name);
    let items_json =
        serde_json::to_value(items).unwrap_or_else(|_| serde_json::Value::Array(vec![]));
    cx.sd().send_to_property_inspector(
        ctx_id,
        serde_json::json!({
            "event": event,
            "items": items_json,
        }),
    );
}

/// Reply to a `getStyles` datasource request.
pub fn reply_styles(cx: &Context, req: &DataSourceRequest<'_>) {
    let mut items = vec![DataSourceResultItem::Item(DataSourceItem {
        disabled: None,
        label: Some("\u{2014} global default \u{2014}".to_string()),
        value: String::new(),
    })];

    if let Some(styles) = cx.try_ext::<StylesState>() {
        for (id, name) in styles.list() {
            items.push(DataSourceResultItem::Item(DataSourceItem {
                disabled: None,
                label: Some(name),
                value: id,
            }));
        }
    }

    cx.sdpi().reply(req, items);
}

/// Reply to a `getFonts` datasource request.
pub fn reply_fonts(cx: &Context, req: &DataSourceRequest<'_>) {
    let mut items = vec![DataSourceResultItem::Item(DataSourceItem {
        disabled: None,
        label: Some("\u{2014} style default \u{2014}".to_string()),
        value: String::new(),
    })];

    if let Some(fonts) = cx.try_ext::<FontsState>() {
        for (id, name) in fonts.list() {
            items.push(DataSourceResultItem::Item(DataSourceItem {
                disabled: None,
                label: Some(name),
                value: id,
            }));
        }
    }

    cx.sdpi().reply(req, items);
}

/// Reply to a `getIcons` datasource request with fuzzy-matched icons first.
///
/// `action_name` and `label` are used for fuzzy matching against icon filenames.
/// Matched icons appear first (sorted by relevance), followed by remaining icons
/// alphabetically.  Pass empty strings to skip matching (pure alphabetical).
pub fn reply_icons(cx: &Context, req: &DataSourceRequest<'_>, action_name: &str, label: &str) {
    let mut items = vec![DataSourceResultItem::Item(DataSourceItem {
        disabled: None,
        label: Some("(none)".to_string()),
        value: String::new(),
    })];

    if let Some(icon_state) = cx.try_ext::<IconFolderState>()
        && let Some(ref folder) = *icon_state.path()
    {
        let files = crate::icons::list_icon_files(folder);
        let matched = if !action_name.is_empty() || !label.is_empty() {
            crate::icons::match_icons(action_name, label, &files)
        } else {
            vec![]
        };

        if !matched.is_empty() {
            for (filename, _score) in &matched {
                items.push(DataSourceResultItem::Item(DataSourceItem {
                    disabled: None,
                    label: Some(filename.clone()),
                    value: filename.clone(),
                }));
            }
            let matched_set: std::collections::HashSet<&str> =
                matched.iter().map(|(f, _)| f.as_str()).collect();
            let mut remaining: Vec<_> = files
                .iter()
                .filter(|f| !matched_set.contains(f.as_str()))
                .collect();
            remaining.sort();
            for filename in remaining {
                items.push(DataSourceResultItem::Item(DataSourceItem {
                    disabled: None,
                    label: Some(filename.clone()),
                    value: filename.clone(),
                }));
            }
        } else {
            let mut sorted = files;
            sorted.sort();
            for filename in sorted {
                items.push(DataSourceResultItem::Item(DataSourceItem {
                    disabled: None,
                    label: Some(filename.clone()),
                    value: filename,
                }));
            }
        }
    }

    cx.sdpi().reply(req, items);
}

// ── Binding display ─────────────────────────────────────────────────────────────

/// Format a binding for human-readable display in the PI.
///
/// Strips the `keyboard+`/`mouse+` device prefix and joins modifiers:
/// - `keyboard+lshift` → `LShift`
/// - `keyboard+f2` with modifiers `[ralt, rctrl]` → `RAlt+RCtrl+F2`
pub fn format_binding_display(b: &Binding) -> String {
    let key = b
        .input
        .strip_prefix("keyboard+")
        .or_else(|| b.input.strip_prefix("mouse+"))
        .unwrap_or(&b.input);

    let display_key = humanize_key(key);

    if b.modifiers.is_empty() {
        display_key
    } else {
        let mods: Vec<String> = b.modifiers.iter().map(|m| humanize_key(m)).collect();
        format!("{}+{}", mods.join("+"), display_key)
    }
}

/// Capitalize a SC key name for display: `lshift` → `LShift`, `f2` → `F2`, `np_7` → `NP7`.
pub fn humanize_key(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "lshift" => "LShift".into(),
        "rshift" => "RShift".into(),
        "lctrl" => "LCtrl".into(),
        "rctrl" => "RCtrl".into(),
        "lalt" => "LAlt".into(),
        "ralt" => "RAlt".into(),
        "space" => "Space".into(),
        "enter" | "return" => "Enter".into(),
        "escape" | "esc" => "Esc".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "delete" | "del" => "Del".into(),
        "insert" | "ins" => "Ins".into(),
        "capslock" => "CapsLock".into(),
        "numlock" => "NumLock".into(),
        "scrolllock" => "ScrollLock".into(),
        "pgup" | "pageup" => "PgUp".into(),
        "pgdn" | "pagedown" => "PgDn".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "printscreen" | "print" => "PrtSc".into(),
        "pause" => "Pause".into(),
        "minus" => "-".into(),
        "equals" | "equal" => "=".into(),
        "comma" => ",".into(),
        "period" | "dot" => ".".into(),
        "slash" => "/".into(),
        "backslash" => "\\".into(),
        "semicolon" => ";".into(),
        "apostrophe" | "quote" => "'".into(),
        "grave" | "tilde" => "`".into(),
        "lbracket" => "[".into(),
        "rbracket" => "]".into(),
        s if s.starts_with("np_") => format!("NP{}", &name[3..]),
        s if s.starts_with("numpad_") => format!("NP{}", &name[7..]),
        s if s.starts_with("mouse") => name.to_uppercase(),
        s if s.starts_with("button") => name.to_uppercase(),
        s if s.starts_with("mwheel_") => {
            if s.ends_with("up") {
                "ScrollUp".into()
            } else {
                "ScrollDown".into()
            }
        }
        _ => name.to_uppercase(),
    }
}

// ── Label resolution ────────────────────────────────────────────────────────────

/// Resolve a map's display label from bindings, falling back to humanized name.
pub fn resolve_map_label(bindings_state: &Option<Arc<BindingsState>>, map_name: &str) -> String {
    bindings_state
        .as_ref()
        .and_then(|bs| {
            bs.with_bindings(|bindings| {
                bindings
                    .action_maps
                    .iter()
                    .find(|m| m.name.as_ref() == map_name)
                    .map(|m| m.ui_label.to_string())
            })
        })
        .flatten()
        .unwrap_or_else(|| crate::bindings::translations::humanize_label(map_name))
}

/// Resolve an action's display label from bindings, falling back to humanized name.
pub fn resolve_action_label(
    bindings_state: &Option<Arc<BindingsState>>,
    map_name: &str,
    action_name: &str,
) -> String {
    bindings_state
        .as_ref()
        .and_then(|bs| {
            bs.with_bindings(|bindings| {
                bindings
                    .action_maps
                    .iter()
                    .filter(|m| m.name.as_ref() == map_name)
                    .flat_map(|m| &m.actions)
                    .find(|a| a.name.as_ref() == action_name)
                    .map(|a| a.ui_label.to_string())
            })
        })
        .flatten()
        .unwrap_or_else(|| crate::bindings::translations::humanize_label(action_name))
}
