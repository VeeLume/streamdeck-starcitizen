use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use constcat::concat;
use streamdeck_lib::input::{InputBusExt, InputCombo};
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::{debug, warn};

use crate::PLUGIN_ID;
use crate::bindings::model::{GameAction, ParsedBindings};
use crate::render;
use crate::state::bindings::BindingsState;
use crate::state::icon_folder::IconFolderState;
use crate::state::styles::StylesState;
use crate::state::toggle_groups::ToggleGroupsState;
use crate::topics;

use super::shared;

// ── Constants ───────────────────────────────────────────────────────────────────

/// How long the user must hold the key (Toggle mode only) to trigger a
/// visual-only re-sync without firing any game binding.
const RESYNC_HOLD_MS: u64 = 800;

// ── Action ──────────────────────────────────────────────────────────────────────

pub struct ToggleAction {
    // Group selection (from TOML)
    group: String, // "map.toggle" compound key, or empty for Custom

    // Resolved action names (from group or overrides)
    toggle_map: String,
    toggle_action: String,
    enable_map: String,
    enable_action: String,
    disable_map: String,
    disable_action: String,

    // Display (per-state)
    on_title: String,
    off_title: String,
    on_icon: String,
    off_icon: String,
    key_style: String,
    key_font: String,
    start_on: bool,
    /// When true, the state label mapping is inverted (state[1] = ON, state[0] = OFF).
    states_swapped: bool,

    // Runtime
    is_on: bool,
    key_down_at: Option<Instant>,
    long_press_cancel: Option<Arc<AtomicBool>>,
    long_press_fired: Arc<AtomicBool>,
}

impl Default for ToggleAction {
    fn default() -> Self {
        Self {
            group: String::new(),
            toggle_map: String::new(),
            toggle_action: String::new(),
            enable_map: String::new(),
            enable_action: String::new(),
            disable_map: String::new(),
            disable_action: String::new(),
            on_title: String::new(),
            off_title: String::new(),
            on_icon: String::new(),
            off_icon: String::new(),
            key_style: String::new(),
            key_font: String::new(),
            start_on: false,
            states_swapped: false,
            is_on: false,
            key_down_at: None,
            long_press_cancel: None,
            long_press_fired: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ToggleAction {
    /// Whether any enable or disable action is configured.
    ///
    /// When true, we use idempotent actions where available and fall back to
    /// toggle for the missing direction. This is better than pure toggle mode
    /// because at least one direction is guaranteed to be in sync.
    fn has_smart_actions(&self) -> bool {
        !self.enable_action.is_empty() || !self.disable_action.is_empty()
    }
}

impl ActionStatic for ToggleAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".toggle-action");
}

impl Action for ToggleAction {
    fn id(&self) -> &str {
        Self::ID
    }

    fn topics(&self) -> &'static [&'static str] {
        &[
            topics::BINDINGS_RELOADED.name,
            topics::ICON_FOLDER_CHANGED.name,
            topics::STYLE_CHANGED.name,
        ]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) {
        debug!("ToggleAction will_appear: {}", ev.context);
        self.apply_settings(cx, ev.settings);

        // If no persisted isOn state, use the start_on default
        if ev.settings.get("isOn").is_none() {
            self.is_on = self.start_on;
        }

        self.render_button(cx, ev.context);
    }

    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) {
        let old_group = self.group.clone();
        let old_toggle_map = self.toggle_map.clone();
        let old_enable_map = self.enable_map.clone();
        let old_disable_map = self.disable_map.clone();

        self.apply_settings(cx, ev.settings);

        // Group changed — clear overrides so the TOML values take effect,
        // then re-resolve the group.
        if self.group != old_group {
            self.toggle_map.clear();
            self.toggle_action.clear();
            self.enable_map.clear();
            self.enable_action.clear();
            self.disable_map.clear();
            self.disable_action.clear();
            self.states_swapped = false;
            self.resolve_group(cx);
            self.save_settings(cx, ev.context);
            self.send_ui_state(cx, ev.context);
        }

        self.render_button(cx, ev.context);
        self.send_action_info(cx, ev.context);

        // Push updated action lists when a category changes
        if self.toggle_map != old_toggle_map {
            self.push_action_items(cx, ev.context, "getToggleActions", &self.toggle_map.clone());
        }
        if self.enable_map != old_enable_map {
            self.push_action_items(cx, ev.context, "getEnableActions", &self.enable_map.clone());
        }
        if self.disable_map != old_disable_map {
            self.push_action_items(
                cx,
                ev.context,
                "getDisableActions",
                &self.disable_map.clone(),
            );
        }
    }

    fn property_inspector_did_appear(&mut self, cx: &Context, ev: &PropertyInspectorDidAppear) {
        debug!("ToggleAction PI appeared: {}", ev.context);
        self.send_ui_state(cx, ev.context);
        self.send_action_info(cx, ev.context);

        // Pre-populate override action dropdowns
        if !self.toggle_map.is_empty() {
            self.push_action_items(cx, ev.context, "getToggleActions", &self.toggle_map.clone());
        }
        if !self.enable_map.is_empty() {
            self.push_action_items(cx, ev.context, "getEnableActions", &self.enable_map.clone());
        }
        if !self.disable_map.is_empty() {
            self.push_action_items(
                cx,
                ev.context,
                "getDisableActions",
                &self.disable_map.clone(),
            );
        }
    }

    fn did_receive_property_inspector_message(
        &mut self,
        cx: &Context,
        ev: &DidReceivePropertyInspectorMessage,
        is_sdpi: bool,
    ) {
        if is_sdpi {
            return;
        }

        let Some(action) = ev.payload.get("action").and_then(|v| v.as_str()) else {
            return;
        };

        match action {
            "getActionInfo" => {
                self.send_ui_state(cx, ev.context);
                self.send_action_info(cx, ev.context);
            }
            "setStartOn" => {
                self.start_on = ev
                    .payload
                    .get("startOn")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.save_settings(cx, ev.context);
            }
            "swapOnOff" => {
                std::mem::swap(&mut self.enable_map, &mut self.disable_map);
                std::mem::swap(&mut self.enable_action, &mut self.disable_action);
                std::mem::swap(&mut self.on_title, &mut self.off_title);
                std::mem::swap(&mut self.on_icon, &mut self.off_icon);
                self.states_swapped = !self.states_swapped;
                self.save_settings(cx, ev.context);
                self.send_ui_state(cx, ev.context);
                self.send_action_info(cx, ev.context);
                self.render_button(cx, ev.context);
            }
            _ => {}
        }
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        debug!(
            "ToggleAction key_down has_smart={}",
            self.has_smart_actions()
        );

        if self.has_smart_actions() {
            // Smart/hybrid mode: fire immediately on key_down.
            // Use idempotent enable/disable where available, fall back to toggle.

            // Multi-action: honor user_desired_state
            if *ev.is_in_multi_action {
                if let Some(&desired) = ev.user_desired_state {
                    self.is_on = desired == 1;
                } else {
                    self.is_on = !self.is_on;
                }
            } else {
                self.is_on = !self.is_on;
            }

            self.fire_for_state(cx);
            self.update_state(cx, ev.context);
        } else {
            // Toggle-only mode: start long-press timer for re-sync
            self.key_down_at = Some(Instant::now());
            self.long_press_fired.store(false, Ordering::SeqCst);

            let cancel = Arc::new(AtomicBool::new(false));
            self.long_press_cancel = Some(cancel.clone());
            let fired = self.long_press_fired.clone();
            let ctx_id = ev.context.to_string();
            let sd = cx.sd().clone();

            let will_be_on = !self.is_on;

            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(RESYNC_HOLD_MS));
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                debug!("Toggle re-sync fired (visual only)");
                fired.store(true, Ordering::SeqCst);
                sd.set_state(&ctx_id, if will_be_on { 1 } else { 0 });
            });
        }
    }

    fn key_up(&mut self, cx: &Context, ev: &KeyUp) {
        if self.has_smart_actions() {
            return;
        }

        // Toggle-only mode: cancel long-press timer
        if let Some(cancel) = self.long_press_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }

        let Some(_down_at) = self.key_down_at.take() else {
            return;
        };

        // If long-press re-sync already fired, apply the state flip
        if self.long_press_fired.load(Ordering::SeqCst) {
            self.long_press_fired.store(false, Ordering::SeqCst);
            self.is_on = !self.is_on;
            self.render_button(cx, ev.context);
            self.save_is_on(cx, ev.context);
            return;
        }

        // Normal short press: fire toggle binding and flip state
        if *ev.is_in_multi_action {
            if let Some(&desired) = ev.user_desired_state {
                self.is_on = desired == 1;
            } else {
                self.is_on = !self.is_on;
            }
        } else {
            self.is_on = !self.is_on;
        }

        self.fire_toggle(cx);
        self.update_state(cx, ev.context);
    }

    fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
        match req.event {
            "getGroups" => self.reply_groups(cx, req),
            "getToggleCategories" | "getEnableCategories" | "getDisableCategories" => {
                self.reply_categories(cx, req);
            }
            "getToggleActions" => {
                self.reply_actions(cx, req, &self.toggle_map.clone());
            }
            "getEnableActions" => {
                self.reply_actions(cx, req, &self.enable_map.clone());
            }
            "getDisableActions" => {
                self.reply_actions(cx, req, &self.disable_map.clone());
            }
            "getOnIcons" | "getOffIcons" => {
                let label = self.derive_label(cx, true);
                shared::reply_icons(cx, req, &self.toggle_action, &label);
            }
            "getStyles" => shared::reply_styles(cx, req),
            "getFonts" => shared::reply_fonts(cx, req),
            _ => {}
        }
    }

    fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
        if event.downcast(topics::BINDINGS_RELOADED).is_some()
            || event.downcast(topics::ICON_FOLDER_CHANGED).is_some()
            || event.downcast(topics::STYLE_CHANGED).is_some()
        {
            self.render_button(cx, ctx_id);
        }
    }
}

// ── Private Implementation ──────────────────────────────────────────────────────

impl ToggleAction {
    fn apply_settings(
        &mut self,
        cx: &Context,
        settings: &serde_json::Map<String, serde_json::Value>,
    ) {
        if let Some(v) = settings.get("group").and_then(|v| v.as_str()) {
            self.group = v.to_string();
        }
        if let Some(v) = settings.get("toggleMap").and_then(|v| v.as_str()) {
            self.toggle_map = v.to_string();
        }
        if let Some(v) = settings.get("toggleAction").and_then(|v| v.as_str()) {
            self.toggle_action = v.to_string();
        }
        if let Some(v) = settings.get("enableMap").and_then(|v| v.as_str()) {
            self.enable_map = v.to_string();
        }
        if let Some(v) = settings.get("enableAction").and_then(|v| v.as_str()) {
            self.enable_action = v.to_string();
        }
        if let Some(v) = settings.get("disableMap").and_then(|v| v.as_str()) {
            self.disable_map = v.to_string();
        }
        if let Some(v) = settings.get("disableAction").and_then(|v| v.as_str()) {
            self.disable_action = v.to_string();
        }
        if let Some(v) = settings.get("onTitle").and_then(|v| v.as_str()) {
            self.on_title = v.to_string();
        }
        if let Some(v) = settings.get("offTitle").and_then(|v| v.as_str()) {
            self.off_title = v.to_string();
        }
        if let Some(v) = settings.get("onIcon").and_then(|v| v.as_str()) {
            self.on_icon = v.to_string();
        }
        if let Some(v) = settings.get("offIcon").and_then(|v| v.as_str()) {
            self.off_icon = v.to_string();
        }
        if let Some(v) = settings.get("keyStyle").and_then(|v| v.as_str()) {
            self.key_style = v.to_string();
        }
        if let Some(v) = settings.get("keyFont").and_then(|v| v.as_str()) {
            self.key_font = v.to_string();
        }
        if let Some(v) = settings.get("startOn").and_then(|v| v.as_bool()) {
            self.start_on = v;
        }
        if let Some(v) = settings.get("statesSwapped").and_then(|v| v.as_bool()) {
            self.states_swapped = v;
        }
        if let Some(v) = settings.get("isOn").and_then(|v| v.as_bool()) {
            self.is_on = v;
        }

        // Resolve group → fill action fields if they're empty
        self.resolve_group(cx);
    }

    /// Resolve the TOML group into action fields.
    ///
    /// If a group is selected and the override fields are empty, fill them
    /// from the TOML definition.
    fn resolve_group(&mut self, cx: &Context) {
        if self.group.is_empty() {
            return;
        }

        let Some(tg_state) = cx.try_ext::<ToggleGroupsState>() else {
            return;
        };
        let snap = tg_state.snapshot();
        let Some(g) = snap.get(&self.group) else {
            return;
        };

        // Fill from TOML if override fields are empty
        if self.toggle_action.is_empty() {
            self.toggle_map = g.map.clone();
            self.toggle_action = g.toggle.clone();
        }
        if self.enable_action.is_empty()
            && let Some(ref on) = g.on
        {
            self.enable_map = g.map.clone();
            self.enable_action = on.clone();
        }
        if self.disable_action.is_empty()
            && let Some(ref off) = g.off
        {
            self.disable_map = g.map.clone();
            self.disable_action = off.clone();
        }
    }

    // ── State management ────────────────────────────────────────────────────────

    fn update_state(&mut self, cx: &Context, ctx_id: &str) {
        cx.sd().set_state(ctx_id, if self.is_on { 1 } else { 0 });
        self.render_button(cx, ctx_id);
        self.save_is_on(cx, ctx_id);
    }

    fn save_is_on(&self, cx: &Context, ctx_id: &str) {
        let mut map = serde_json::Map::new();
        self.write_all_settings(&mut map);
        cx.sd().set_settings(ctx_id, map);
    }

    // ── Binding execution ───────────────────────────────────────────────────────

    /// Fire the best available action for the current state.
    ///
    /// - Turning ON:  use enable if available, else toggle
    /// - Turning OFF: use disable if available, else toggle
    fn fire_for_state(&self, cx: &Context) {
        if self.is_on {
            if !self.enable_action.is_empty() {
                self.fire_enable(cx);
            } else {
                self.fire_toggle(cx);
            }
        } else if !self.disable_action.is_empty() {
            self.fire_disable(cx);
        } else {
            self.fire_toggle(cx);
        }
    }

    fn fire_toggle(&self, cx: &Context) {
        if let Some(combo) = self.resolve_combo(cx, &self.toggle_map, &self.toggle_action) {
            debug!("Firing toggle: {combo}");
            cx.bus().execute(combo);
        } else {
            warn!(
                "No keyboard binding for {}.{}",
                self.toggle_map, self.toggle_action
            );
        }
    }

    fn fire_enable(&self, cx: &Context) {
        if let Some(combo) = self.resolve_combo(cx, &self.enable_map, &self.enable_action) {
            debug!("Firing enable: {combo}");
            cx.bus().execute(combo);
        } else {
            warn!(
                "No keyboard binding for {}.{}",
                self.enable_map, self.enable_action
            );
        }
    }

    fn fire_disable(&self, cx: &Context) {
        if let Some(combo) = self.resolve_combo(cx, &self.disable_map, &self.disable_action) {
            debug!("Firing disable: {combo}");
            cx.bus().execute(combo);
        } else {
            warn!(
                "No keyboard binding for {}.{}",
                self.disable_map, self.disable_action
            );
        }
    }

    fn resolve_combo(&self, cx: &Context, map_name: &str, action_name: &str) -> Option<InputCombo> {
        shared::resolve_combo(cx, map_name, action_name)
    }

    // ── Rendering ───────────────────────────────────────────────────────────────

    fn render_button(&self, cx: &Context, ctx_id: &str) {
        self.render_state_image(cx, ctx_id, false, 0);
        self.render_state_image(cx, ctx_id, true, 1);
        cx.sd().set_state(ctx_id, if self.is_on { 1 } else { 0 });
    }

    fn render_state_image(&self, cx: &Context, ctx_id: &str, is_on: bool, state_index: u8) {
        let icon_file = if is_on { &self.on_icon } else { &self.off_icon };
        if !icon_file.is_empty()
            && let Some(icon_state) = cx.try_ext::<IconFolderState>()
            && let Some(ref folder) = *icon_state.path()
        {
            let icon_path = folder.join(icon_file);
            if icon_path.exists()
                && let Ok(data) = std::fs::read(&icon_path)
            {
                let ext = icon_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_lowercase();
                let mime = if ext == "svg" {
                    "image/svg+xml"
                } else {
                    "image/png"
                };
                use base64::{Engine, engine::general_purpose::STANDARD};
                let b64 = STANDARD.encode(&data);
                let data_url = format!("data:{mime};base64,{b64}");
                cx.sd()
                    .set_image(ctx_id, Some(data_url), Some(state_index), None);
                return;
            }
        }

        let label = self.derive_label(cx, is_on);

        let mut style = if let Some(styles) = cx.try_ext::<StylesState>() {
            crate::state::styles::resolve_style(&self.key_style, &styles, &cx.globals())
        } else {
            crate::styles::style_default()
        };
        if !self.key_font.is_empty() {
            style.font = self.key_font.clone();
        }
        let custom_title = if is_on {
            &self.on_title
        } else {
            &self.off_title
        };
        if !custom_title.is_empty() {
            style.abbreviate = false;
        }

        render::render_toggle(cx, ctx_id, &label, is_on, &style, state_index);
    }

    fn derive_label(&self, cx: &Context, is_on: bool) -> String {
        let custom_title = if is_on {
            &self.on_title
        } else {
            &self.off_title
        };
        if !custom_title.is_empty() {
            return custom_title.clone();
        }

        if self.toggle_action.is_empty() {
            return if is_on {
                "ON".to_string()
            } else {
                "OFF".to_string()
            };
        }

        // Derive the base label from the toggle action and append the state.
        let (base_label, state_label) = if let Some(bindings_state) = cx.try_ext::<BindingsState>()
        {
            bindings_state
                .with_bindings(|bindings| {
                    let action =
                        self.find_game_action(bindings, &self.toggle_map, &self.toggle_action)?;

                    let base = strip_toggle_prefix(&action.ui_label);

                    // Use parsed <states> if available (e.g. "locked"/"unlocked"),
                    // otherwise fall back to "On"/"Off".
                    // When swapped, invert the index so the label matches the new meaning.
                    let state_idx = match (is_on, self.states_swapped) {
                        (true, false) | (false, true) => 0,
                        (false, false) | (true, true) => 1,
                    };
                    let state = action
                        .states
                        .get(state_idx)
                        .map(|s| capitalize_first(&s.name))
                        .unwrap_or_else(|| {
                            if is_on {
                                "On".to_string()
                            } else {
                                "Off".to_string()
                            }
                        });

                    Some((base.to_string(), state))
                })
                .flatten()
                .unwrap_or_else(|| {
                    let base = crate::bindings::translations::humanize_label(&self.toggle_action);
                    let state = if is_on {
                        "On".to_string()
                    } else {
                        "Off".to_string()
                    };
                    (base, state)
                })
        } else {
            let base = crate::bindings::translations::humanize_label(&self.toggle_action);
            let state = if is_on {
                "On".to_string()
            } else {
                "Off".to_string()
            };
            (base, state)
        };

        format!("{base_label}\\n{state_label}")
    }

    fn find_game_action<'a>(
        &self,
        bindings: &'a ParsedBindings,
        map_name: &str,
        action_name: &str,
    ) -> Option<&'a GameAction> {
        bindings
            .action_maps
            .iter()
            .filter(|m| m.name.as_ref() == map_name)
            .flat_map(|m| &m.actions)
            .find(|a| a.name.as_ref() == action_name)
    }

    // ── Settings persistence ────────────────────────────────────────────────────

    fn save_settings(&self, cx: &Context, ctx_id: &str) {
        let mut map = serde_json::Map::new();
        self.write_all_settings(&mut map);
        cx.sd().set_settings(ctx_id, map);
    }

    fn write_all_settings(&self, map: &mut serde_json::Map<String, serde_json::Value>) {
        map.insert("group".into(), self.group.clone().into());
        map.insert("toggleMap".into(), self.toggle_map.clone().into());
        map.insert("toggleAction".into(), self.toggle_action.clone().into());
        map.insert("enableMap".into(), self.enable_map.clone().into());
        map.insert("enableAction".into(), self.enable_action.clone().into());
        map.insert("disableMap".into(), self.disable_map.clone().into());
        map.insert("disableAction".into(), self.disable_action.clone().into());
        map.insert("onTitle".into(), self.on_title.clone().into());
        map.insert("offTitle".into(), self.off_title.clone().into());
        map.insert("onIcon".into(), self.on_icon.clone().into());
        map.insert("offIcon".into(), self.off_icon.clone().into());
        map.insert("keyStyle".into(), self.key_style.clone().into());
        map.insert("keyFont".into(), self.key_font.clone().into());
        map.insert("startOn".into(), self.start_on.into());
        map.insert("statesSwapped".into(), self.states_swapped.into());
        map.insert("isOn".into(), self.is_on.into());
    }

    // ── PI state push ───────────────────────────────────────────────────────────

    fn send_ui_state(&self, cx: &Context, ctx_id: &str) {
        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "uiState",
                "group": self.group,
                "isSmart": self.has_smart_actions(),
                "startOn": self.start_on,
                "isOn": self.is_on,
            }),
        );
    }

    fn send_action_info(&self, cx: &Context, ctx_id: &str) {
        let (toggle_label, toggle_bind) =
            self.describe_action(cx, &self.toggle_map, &self.toggle_action);
        let (enable_label, enable_bind) =
            self.describe_action(cx, &self.enable_map, &self.enable_action);
        let (disable_label, disable_bind) =
            self.describe_action(cx, &self.disable_map, &self.disable_action);

        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "actionInfo",
                "toggleLabel": toggle_label,
                "toggleCategory": self.toggle_map,
                "toggleId": self.toggle_action,
                "toggleBind": toggle_bind,
                "enableLabel": enable_label,
                "enableCategory": self.enable_map,
                "enableId": self.enable_action,
                "enableBind": enable_bind,
                "disableLabel": disable_label,
                "disableCategory": self.disable_map,
                "disableId": self.disable_action,
                "disableBind": disable_bind,
            }),
        );
    }

    fn describe_action(&self, cx: &Context, map_name: &str, action_name: &str) -> (String, String) {
        shared::describe_action(cx, map_name, action_name)
    }

    // ── Datasource: push ──────────────────────────────────────────────────────

    fn push_action_items(&self, cx: &Context, ctx_id: &str, event: &str, map_name: &str) {
        shared::push_action_items(cx, ctx_id, event, map_name);
    }

    // ── Datasource: groups ──────────────────────────────────────────────────────

    /// Reply with all TOML groups organized by category for the PI dropdown.
    ///
    /// Groups are ordered by the actionmap order from `ParsedBindings` (matching
    /// the in-game keybinding UI), not the TOML file order.
    fn reply_groups(&self, cx: &Context, req: &DataSourceRequest<'_>) {
        let Some(tg_state) = cx.try_ext::<ToggleGroupsState>() else {
            cx.sdpi().reply(req, vec![]);
            return;
        };
        let snap = tg_state.snapshot();
        let bindings_state = cx.try_ext::<BindingsState>();

        // Index TOML groups by map name for quick lookup
        let mut groups_by_map: std::collections::HashMap<
            &str,
            Vec<&crate::state::toggle_groups::ToggleGroup>,
        > = std::collections::HashMap::new();
        for g in snap.iter() {
            groups_by_map.entry(g.map.as_str()).or_default().push(g);
        }

        let mut items: Vec<DataSourceResultItem> = Vec::new();

        // Walk actionmaps in their parsed order (matches in-game UI order)
        if let Some(ref bs) = bindings_state {
            bs.with_bindings(|bindings| {
                let mut seen_maps = std::collections::HashSet::new();
                for am in &bindings.action_maps {
                    // Deduplicate by map name (same map can appear multiple times)
                    if !seen_maps.insert(am.name.as_ref()) {
                        continue;
                    }
                    let Some(map_groups) = groups_by_map.remove(am.name.as_ref()) else {
                        continue;
                    };

                    let mut children: Vec<DataSourceItem> = Vec::new();
                    for g in map_groups {
                        let toggle_label =
                            shared::resolve_action_label(&bindings_state, &g.map, &g.toggle);
                        let key = format!("{}.{}", g.map, g.toggle);
                        let has_on_off = g.on.is_some() || g.off.is_some();
                        let suffix = if has_on_off { "" } else { " (toggle only)" };

                        children.push(DataSourceItem {
                            disabled: None,
                            label: Some(format!("{toggle_label}{suffix}")),
                            value: key,
                        });
                    }

                    if !children.is_empty() {
                        items.push(DataSourceResultItem::Group(DataSourceGroup {
                            label: Some(am.ui_label.to_string()),
                            children,
                        }));
                    }
                }
            });
        }

        // Append any remaining TOML groups whose maps weren't in bindings
        // (shouldn't happen normally, but handles edge cases)
        for (map_name, map_groups) in groups_by_map {
            let mut children: Vec<DataSourceItem> = Vec::new();
            for g in map_groups {
                let toggle_label = shared::resolve_action_label(&bindings_state, &g.map, &g.toggle);
                let key = format!("{}.{}", g.map, g.toggle);
                children.push(DataSourceItem {
                    disabled: None,
                    label: Some(toggle_label),
                    value: key,
                });
            }
            if !children.is_empty() {
                let map_label = shared::resolve_map_label(&bindings_state, map_name);
                items.push(DataSourceResultItem::Group(DataSourceGroup {
                    label: Some(map_label),
                    children,
                }));
            }
        }

        // "Custom..." as a standalone item
        items.push(DataSourceResultItem::Item(DataSourceItem {
            disabled: None,
            label: Some("Custom...".to_string()),
            value: String::new(),
        }));

        cx.sdpi().reply(req, items);
    }

    // ── Datasource: categories & actions (for override section) ─────────────────

    fn reply_categories(&self, cx: &Context, req: &DataSourceRequest<'_>) {
        cx.sdpi().reply(req, shared::get_category_items(cx));
    }

    fn reply_actions(&self, cx: &Context, req: &DataSourceRequest<'_>, selected_map: &str) {
        cx.sdpi()
            .reply(req, shared::get_action_items(cx, selected_map));
    }
}

// ── Label helpers ───────────────────────────────────────────────────────────────

/// Strip common toggle/set/enable/disable prefixes from an action label.
///
/// "Toggle Power - Thrusters" → "Power - Thrusters"
/// "Set Thrusters Power On"   → "Thrusters Power On"
/// "Toggle Mining Mode"       → "Mining Mode"
fn strip_toggle_prefix(label: &str) -> &str {
    let prefixes = [
        "Toggle ",
        "Set ",
        "Enable ",
        "Disable ",
        "Activate ",
        "Deactivate ",
    ];
    for prefix in prefixes {
        if let Some(rest) = label.strip_prefix(prefix) {
            return rest;
        }
    }
    label
}

/// Capitalize the first character of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
