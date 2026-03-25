use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use constcat::concat;
use streamdeck_lib::input::{InputBusExt, InputCombo};
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::{debug, warn};

use crate::PLUGIN_ID;
use crate::bindings::HIDDEN_ACTION_MAPS;
use crate::bindings::executor::binding_to_combo;
use crate::bindings::model::{Binding, Device, GameAction, ParsedBindings};
use crate::render;
use crate::state::bindings::BindingsState;
use crate::state::fonts::FontsState;
use crate::state::icon_folder::IconFolderState;
use crate::state::styles::StylesState;
use crate::topics;

// ── Constants ───────────────────────────────────────────────────────────────────

const DEFAULT_HOLD_THRESHOLD_MS: u64 = 400;
const DEFAULT_DOUBLE_WINDOW_MS: u64 = 300;

// ── Action ──────────────────────────────────────────────────────────────────────

pub struct ExecuteAction {
    // Settings from PI
    primary_map: String,
    primary_action: String,
    hold_map: String,
    hold_action: String,
    double_map: String,
    double_action: String,
    hold_enabled: bool,
    double_enabled: bool,
    hold_threshold_ms: u64,
    double_window_ms: u64,
    custom_title: String,
    icon_file: String,
    key_style: String,
    key_font: String,

    // Runtime state for press detection
    key_down_at: Option<Instant>,
    last_up_at: Option<Instant>,
    awaiting_double: bool,
    hold_cancel: Option<Arc<AtomicBool>>,
    hold_fired: Arc<AtomicBool>,
}

impl Default for ExecuteAction {
    fn default() -> Self {
        Self {
            primary_map: String::new(),
            primary_action: String::new(),
            hold_map: String::new(),
            hold_action: String::new(),
            double_map: String::new(),
            double_action: String::new(),
            hold_enabled: false,
            double_enabled: false,
            hold_threshold_ms: DEFAULT_HOLD_THRESHOLD_MS,
            double_window_ms: DEFAULT_DOUBLE_WINDOW_MS,
            custom_title: String::new(),
            icon_file: String::new(),
            key_style: String::new(),
            key_font: String::new(),
            key_down_at: None,
            last_up_at: None,
            awaiting_double: false,
            hold_cancel: None,
            hold_fired: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ActionStatic for ExecuteAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".execute-action");
}

impl Action for ExecuteAction {
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
        debug!("ExecuteAction will_appear: {}", ev.context);
        self.apply_settings(ev.settings);
        self.render_button(cx, ev.context);
    }

    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) {
        let old_primary_map = self.primary_map.clone();
        let old_hold_map = self.hold_map.clone();
        let old_double_map = self.double_map.clone();

        self.apply_settings(ev.settings);
        self.render_button(cx, ev.context);
        self.send_action_info(cx, ev.context);

        // When a category changes, push the new action list to the PI.
        // sdpi-components will update the corresponding <sdpi-select datasource="...">
        if self.primary_map != old_primary_map {
            self.push_action_items(cx, ev.context, "getActions", &self.primary_map.clone());
        }
        if self.hold_map != old_hold_map {
            self.push_action_items(cx, ev.context, "getHoldActions", &self.hold_map.clone());
        }
        if self.double_map != old_double_map {
            self.push_action_items(cx, ev.context, "getDoubleActions", &self.double_map.clone());
        }
    }

    fn property_inspector_did_appear(&mut self, cx: &Context, ev: &PropertyInspectorDidAppear) {
        debug!("ExecuteAction PI appeared: {}", ev.context);
        self.send_ui_state(cx, ev.context);
        self.send_action_info(cx, ev.context);

        // Pre-populate action dropdowns so they have items on PI open
        if !self.primary_map.is_empty() {
            self.push_action_items(cx, ev.context, "getActions", &self.primary_map.clone());
        }
        if !self.hold_map.is_empty() {
            self.push_action_items(cx, ev.context, "getHoldActions", &self.hold_map.clone());
        }
        if !self.double_map.is_empty() {
            self.push_action_items(cx, ev.context, "getDoubleActions", &self.double_map.clone());
        }
    }

    fn did_receive_property_inspector_message(
        &mut self,
        cx: &Context,
        ev: &DidReceivePropertyInspectorMessage,
        is_sdpi: bool,
    ) {
        // Skip sdpi datasource requests — handled by did_receive_sdpi_request
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
            "setHoldEnabled" => {
                self.hold_enabled = ev
                    .payload
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let Some(ms) = ev.payload.get("thresholdMs").and_then(|v| v.as_u64()) {
                    self.hold_threshold_ms = ms.max(100);
                }
                self.save_settings(cx, ev.context);
                self.send_ui_state(cx, ev.context);
            }
            "setHoldThreshold" => {
                if let Some(ms) = ev.payload.get("ms").and_then(|v| v.as_u64()) {
                    self.hold_threshold_ms = ms.max(100);
                    self.save_settings(cx, ev.context);
                }
            }
            "setDoublePressEnabled" => {
                self.double_enabled = ev
                    .payload
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let Some(ms) = ev.payload.get("windowMs").and_then(|v| v.as_u64()) {
                    self.double_window_ms = ms.max(100);
                }
                self.save_settings(cx, ev.context);
                self.send_ui_state(cx, ev.context);
            }
            "setDoublePressWindow" => {
                if let Some(ms) = ev.payload.get("ms").and_then(|v| v.as_u64()) {
                    self.double_window_ms = ms.max(100);
                    self.save_settings(cx, ev.context);
                }
            }
            _ => {}
        }
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        debug!(
            "ExecuteAction key_down primary={}.{}",
            self.primary_map, self.primary_action
        );

        // Fast path: no hold or double-press configured
        if !self.hold_enabled && !self.double_enabled {
            self.fire_primary(cx);
            cx.sd().show_ok(ev.context);
            return;
        }

        // Record the press time and reset hold state
        self.key_down_at = Some(Instant::now());
        self.hold_fired.store(false, Ordering::SeqCst);

        // Check if this is a double-press (second press within window)
        if self.double_enabled && self.awaiting_double {
            if let Some(last_up) = self.last_up_at
                && last_up.elapsed() < Duration::from_millis(self.double_window_ms)
            {
                debug!("Double-press detected");
                self.fire_double(cx);
                self.awaiting_double = false;
                self.last_up_at = None;
                self.key_down_at = None;
                cx.sd().show_ok(ev.context);
                return;
            }
            // Window expired, this is a new press
            self.awaiting_double = false;
        }

        // Start hold timer: fires the hold action after threshold while key is still down
        if self.hold_enabled && !self.hold_action.is_empty() {
            let cancel = Arc::new(AtomicBool::new(false));
            self.hold_cancel = Some(cancel.clone());
            let fired = self.hold_fired.clone();

            if let Some(combo) = self.resolve_combo(cx, &self.hold_map, &self.hold_action) {
                let threshold = self.hold_threshold_ms;
                let ctx_id = ev.context.to_string();
                let sd = cx.sd().clone();
                let bus = cx.bus();

                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(threshold));
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    debug!("Hold fired (threshold {threshold}ms)");
                    bus.execute(combo);
                    fired.store(true, Ordering::SeqCst);
                    sd.show_ok(&ctx_id);
                });
            }
        }
    }

    fn key_up(&mut self, cx: &Context, ev: &KeyUp) {
        // Cancel any pending hold timer
        if let Some(cancel) = self.hold_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }

        let Some(_down_at) = self.key_down_at.take() else {
            return;
        };

        // If hold already fired during the press, we're done
        if self.hold_fired.load(Ordering::SeqCst) {
            self.hold_fired.store(false, Ordering::SeqCst);
            self.awaiting_double = false;
            return;
        }

        // Double-press: if enabled, start the waiting window
        if self.double_enabled {
            self.awaiting_double = true;
            self.last_up_at = Some(Instant::now());

            // Spawn a timer to fire primary if no second press arrives.
            let bus = cx.bus();
            let combo = self.resolve_primary_combo(cx);
            let window = self.double_window_ms;
            let ctx_id = ev.context.to_string();
            let sd = cx.sd().clone();

            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(window + 20));
                // If we get here, no double-press occurred — fire primary
                if let Some(combo) = combo {
                    bus.execute(combo);
                    sd.show_ok(&ctx_id);
                }
            });
            return;
        }

        // No hold/double, just quick release → fire primary
        self.fire_primary(cx);
        cx.sd().show_ok(ev.context);
    }

    fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
        match req.event {
            "getCategories" | "getHoldCategories" | "getDoubleCategories" => {
                self.reply_categories(cx, req);
            }
            "getActions" | "getHoldActions" | "getDoubleActions" => {
                let map_name = self.map_for_datasource(req.event, req);
                self.reply_actions(cx, req, &map_name);
            }
            "getIcons" => {
                self.reply_icons(cx, req);
            }
            "getStyles" => {
                reply_styles(cx, req);
            }
            "getFonts" => {
                reply_fonts(cx, req);
            }
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

impl ExecuteAction {
    fn apply_settings(&mut self, settings: &serde_json::Map<String, serde_json::Value>) {
        if let Some(v) = settings.get("primaryMap").and_then(|v| v.as_str()) {
            self.primary_map = v.to_string();
        }
        if let Some(v) = settings.get("primaryAction").and_then(|v| v.as_str()) {
            self.primary_action = v.to_string();
        }
        if let Some(v) = settings.get("holdMap").and_then(|v| v.as_str()) {
            self.hold_map = v.to_string();
        }
        if let Some(v) = settings.get("holdAction").and_then(|v| v.as_str()) {
            self.hold_action = v.to_string();
        }
        if let Some(v) = settings.get("doubleMap").and_then(|v| v.as_str()) {
            self.double_map = v.to_string();
        }
        if let Some(v) = settings.get("doubleAction").and_then(|v| v.as_str()) {
            self.double_action = v.to_string();
        }
        if let Some(v) = settings.get("holdEnabled").and_then(|v| v.as_bool()) {
            self.hold_enabled = v;
        }
        if let Some(v) = settings.get("doubleEnabled").and_then(|v| v.as_bool()) {
            self.double_enabled = v;
        }
        if let Some(v) = settings.get("holdThreshold").and_then(|v| v.as_u64()) {
            self.hold_threshold_ms = v.max(100);
        }
        if let Some(v) = settings.get("doubleWindow").and_then(|v| v.as_u64()) {
            self.double_window_ms = v.max(100);
        }
        if let Some(v) = settings.get("customTitle").and_then(|v| v.as_str()) {
            self.custom_title = v.to_string();
        }
        if let Some(v) = settings.get("iconFile").and_then(|v| v.as_str()) {
            self.icon_file = v.to_string();
        }
        if let Some(v) = settings.get("keyStyle").and_then(|v| v.as_str()) {
            self.key_style = v.to_string();
        }
        if let Some(v) = settings.get("keyFont").and_then(|v| v.as_str()) {
            self.key_font = v.to_string();
        }
    }

    fn render_button(&self, cx: &Context, ctx_id: &str) {
        // Priority: custom icon → custom title → auto-derived label → placeholder
        if !self.icon_file.is_empty()
            && let Some(icon_state) = cx.try_ext::<IconFolderState>()
            && let Some(ref folder) = *icon_state.path()
        {
            let icon_path = folder.join(&self.icon_file);
            if icon_path.exists() {
                // Try to load and set the icon image
                if let Ok(data) = std::fs::read(&icon_path) {
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
                    cx.sd().set_image(ctx_id, Some(data_url), None, None);
                    return;
                }
            }
        }

        let label = if !self.custom_title.is_empty() {
            self.custom_title.clone()
        } else if !self.primary_action.is_empty() {
            self.derive_label(cx)
        } else {
            "No Action".to_string()
        };

        let mut style = if let Some(styles) = cx.try_ext::<StylesState>() {
            crate::state::styles::resolve_style(&self.key_style, &styles, &cx.globals())
        } else {
            crate::styles::style_default()
        };
        // Per-key font overrides the style's font
        if !self.key_font.is_empty() {
            style.font = self.key_font.clone();
        }
        // Don't abbreviate user-entered custom titles
        if !self.custom_title.is_empty() {
            style.abbreviate = false;
        }
        render::render_label(cx, ctx_id, &label, &style);
    }

    fn derive_label(&self, cx: &Context) -> String {
        // Try to find the translated label from bindings state
        if let Some(bindings_state) = cx.try_ext::<BindingsState>() {
            let result = bindings_state.with_bindings(|bindings| {
                self.find_game_action(bindings)
                    .map(|a| a.ui_label.to_string())
            });
            if let Some(Some(label)) = result {
                return label;
            }
        }
        // Fallback: humanize the action name
        crate::bindings::translations::humanize_label(&self.primary_action)
    }

    fn find_game_action<'a>(&self, bindings: &'a ParsedBindings) -> Option<&'a GameAction> {
        bindings
            .action_maps
            .iter()
            .filter(|m| m.name.as_ref() == self.primary_map)
            .flat_map(|m| &m.actions)
            .find(|a| a.name.as_ref() == self.primary_action)
    }

    fn resolve_combo(&self, cx: &Context, map_name: &str, action_name: &str) -> Option<InputCombo> {
        let bindings_state = cx.try_ext::<BindingsState>()?;
        bindings_state.with_bindings(|bindings| {
            let action = bindings
                .action_maps
                .iter()
                .filter(|m| m.name.as_ref() == map_name)
                .flat_map(|m| &m.actions)
                .find(|a| a.name.as_ref() == action_name)?;

            // Find first keyboard binding
            let kb_binding = action
                .bindings
                .iter()
                .find(|b| b.device == Device::Keyboard)?;

            binding_to_combo(kb_binding, &bindings.activation_modes, action)
        })?
    }

    fn resolve_primary_combo(&self, cx: &Context) -> Option<InputCombo> {
        self.resolve_combo(cx, &self.primary_map, &self.primary_action)
    }

    fn fire_primary(&self, cx: &Context) {
        if let Some(combo) = self.resolve_primary_combo(cx) {
            debug!("Firing primary: {combo}");
            cx.bus().execute(combo);
        } else {
            warn!(
                "No keyboard binding for {}.{}",
                self.primary_map, self.primary_action
            );
        }
    }

    fn fire_double(&self, cx: &Context) {
        if !self.double_action.is_empty()
            && let Some(combo) = self.resolve_combo(cx, &self.double_map, &self.double_action)
        {
            debug!("Firing double: {combo}");
            cx.bus().execute(combo);
        }
    }

    // ── PI state push ────────────────────────────────────────────────────────────

    fn send_ui_state(&self, cx: &Context, ctx_id: &str) {
        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "uiState",
                "holdEnabled": self.hold_enabled,
                "holdThresholdMs": self.hold_threshold_ms,
                "doublePressEnabled": self.double_enabled,
                "doublePressWindowMs": self.double_window_ms,
            }),
        );
    }

    fn send_action_info(&self, cx: &Context, ctx_id: &str) {
        let (primary_label, primary_bind) =
            self.describe_action(cx, &self.primary_map, &self.primary_action);
        let (hold_label, hold_bind) = self.describe_action(cx, &self.hold_map, &self.hold_action);
        let (double_label, double_bind) =
            self.describe_action(cx, &self.double_map, &self.double_action);

        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "actionInfo",
                "primaryLabel": primary_label,
                "primaryCategory": self.primary_map,
                "primaryId": self.primary_action,
                "primaryBind": primary_bind,
                "holdLabel": hold_label,
                "holdCategory": self.hold_map,
                "holdId": self.hold_action,
                "holdBind": hold_bind,
                "doublePressLabel": double_label,
                "doublePressCategory": self.double_map,
                "doublePressId": self.double_action,
                "doublePressBind": double_bind,
            }),
        );
    }

    fn describe_action(&self, cx: &Context, map_name: &str, action_name: &str) -> (String, String) {
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

    fn save_settings(&self, cx: &Context, ctx_id: &str) {
        let mut map = serde_json::Map::new();
        map.insert("primaryMap".into(), self.primary_map.clone().into());
        map.insert("primaryAction".into(), self.primary_action.clone().into());
        map.insert("holdMap".into(), self.hold_map.clone().into());
        map.insert("holdAction".into(), self.hold_action.clone().into());
        map.insert("doubleMap".into(), self.double_map.clone().into());
        map.insert("doubleAction".into(), self.double_action.clone().into());
        map.insert("holdEnabled".into(), self.hold_enabled.into());
        map.insert("doubleEnabled".into(), self.double_enabled.into());
        map.insert("holdThreshold".into(), self.hold_threshold_ms.into());
        map.insert("doubleWindow".into(), self.double_window_ms.into());
        map.insert("customTitle".into(), self.custom_title.clone().into());
        map.insert("iconFile".into(), self.icon_file.clone().into());
        map.insert("keyStyle".into(), self.key_style.clone().into());
        map.insert("keyFont".into(), self.key_font.clone().into());
        cx.sd().set_settings(ctx_id, map);
    }

    /// Push action items for a datasource event to the PI.
    ///
    /// sdpi-components intercepts `sendToPropertyInspector` messages with
    /// `{"event": "<datasource>", "items": [...]}` and updates the corresponding
    /// `<sdpi-select datasource="...">` dropdown.
    fn push_action_items(&self, cx: &Context, ctx_id: &str, event: &str, map_name: &str) {
        let items = self.get_action_items(cx, map_name);
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

    // ── Datasource helpers ──────────────────────────────────────────────────────

    fn reply_categories(&self, cx: &Context, req: &DataSourceRequest<'_>) {
        let items = self.get_category_items(cx);
        cx.sdpi().reply(req, items);
    }

    fn reply_actions(&self, cx: &Context, req: &DataSourceRequest<'_>, selected_map: &str) {
        let items = self.get_action_items(cx, selected_map);
        cx.sdpi().reply(req, items);
    }

    fn map_for_datasource(&self, event: &str, _req: &DataSourceRequest<'_>) -> String {
        // For "getActions", use primaryMap; for "getHoldActions", use holdMap; etc.
        // The PI sends the current map selection; we read it from our settings.
        match event {
            "getHoldActions" => self.hold_map.clone(),
            "getDoubleActions" => self.double_map.clone(),
            _ => self.primary_map.clone(),
        }
    }

    fn get_category_items(&self, cx: &Context) -> Vec<DataSourceResultItem> {
        let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
            return vec![];
        };
        let result = bindings_state.with_bindings(|bindings| {
            let mut seen = std::collections::HashSet::new();
            bindings
                .action_maps
                .iter()
                .filter(|m| !HIDDEN_ACTION_MAPS.contains(&m.name.as_ref()))
                .filter(|m| {
                    // Deduplicate by UI label (maps with same label are grouped)
                    seen.insert(m.ui_label.as_ref().to_string())
                })
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

    fn reply_icons(&self, cx: &Context, req: &DataSourceRequest<'_>) {
        let mut items = vec![DataSourceResultItem::Item(DataSourceItem {
            disabled: None,
            label: Some("(none)".to_string()),
            value: String::new(),
        })];

        if let Some(icon_state) = cx.try_ext::<IconFolderState>()
            && let Some(ref folder) = *icon_state.path()
        {
            let files = crate::icons::list_icon_files(folder);
            let label = self.derive_label(cx);
            let matched = crate::icons::match_icons(&self.primary_action, &label, &files);

            if !matched.is_empty() {
                // Show matched icons first (sorted by relevance)
                for (filename, _score) in &matched {
                    items.push(DataSourceResultItem::Item(DataSourceItem {
                        disabled: None,
                        label: Some(filename.clone()),
                        value: filename.clone(),
                    }));
                }
                // Then all remaining files alphabetically
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
                // No matches — show all files alphabetically
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

    fn get_action_items(&self, cx: &Context, map_name: &str) -> Vec<DataSourceResultItem> {
        let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
            return vec![];
        };
        let result = bindings_state.with_bindings(|bindings| {
            bindings
                .action_maps
                .iter()
                .filter(|m| m.name.as_ref() == map_name)
                .flat_map(|m| &m.actions)
                .filter(|a| {
                    // Hide _long suffix actions (they're accessed via hold)
                    !a.name.ends_with("_long")
                })
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
}

/// Reply to a `getFonts` datasource request with all available fonts.
fn reply_fonts(cx: &Context, req: &DataSourceRequest<'_>) {
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

/// Reply to a `getStyles` datasource request with all available styles.
fn reply_styles(cx: &Context, req: &DataSourceRequest<'_>) {
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

/// Format a binding for human-readable display in the PI.
///
/// Strips the `keyboard+`/`mouse+` device prefix and joins modifiers:
/// - `keyboard+lshift` → `LShift`
/// - `keyboard+f2` with modifiers `[ralt, rctrl]` → `RAlt+RCtrl+F2`
fn format_binding_display(b: &Binding) -> String {
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
fn humanize_key(name: &str) -> String {
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
