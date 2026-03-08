use std::path::PathBuf;

use constcat::concat;
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::debug;

use crate::PLUGIN_ID;
use crate::render;
use crate::state::bindings::BindingsState;
use crate::state::icon_folder::IconFolderState;
use crate::state::installations::ActiveInstallationState;
use crate::state::styles::StylesState;
use crate::styles::KeyStyle;
use crate::topics;

// ── Global settings keys ────────────────────────────────────────────────────────

const KEY_AUTO_LOAD: &str = "autoLoadBindings";
const KEY_AUTO_SELECT: &str = "autoSelectLastLaunched";
const KEY_ICON_FOLDER: &str = "iconFolder";
const KEY_DEFAULT_STYLE: &str = "defaultKeyStyle";

// ── Action ──────────────────────────────────────────────────────────────────────

pub struct SettingsAction {
    auto_load: bool,
    auto_select_last: bool,
    icon_folder: String,
    default_style: String,
}

impl Default for SettingsAction {
    fn default() -> Self {
        Self {
            auto_load: true,
            auto_select_last: false,
            icon_folder: String::new(),
            default_style: String::new(),
        }
    }
}

impl ActionStatic for SettingsAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".settings");
}

impl Action for SettingsAction {
    fn id(&self) -> &str {
        Self::ID
    }

    fn topics(&self) -> &'static [&'static str] {
        &[
            topics::INSTALLATION_CHANGED.name,
            topics::INSTALLATIONS_REFRESHED.name,
            topics::BINDINGS_RELOAD_REQUESTED.name,
            topics::STYLE_CHANGED.name,
        ]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) {
        debug!("SettingsAction will_appear: {}", ev.context);
        // Hydrate from globals, not per-action settings
        self.hydrate_from_globals(cx);
        self.render_status(cx, ev.context);
    }

    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) {
        let old_style = self.default_style.clone();
        // PI writes to per-action settings → promote to globals
        self.apply_settings(ev.settings);
        self.promote_to_globals(cx);
        self.handle_icon_folder_change(cx);
        if self.default_style != old_style {
            cx.bus()
                .publish_t(topics::STYLE_CHANGED, topics::StyleChanged);
        }
        self.render_status(cx, ev.context);
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        debug!("SettingsAction key_down — refreshing");
        let style = self.resolve_style(cx);
        render::render_progress(cx, ev.context, "Scanning\u{2026}", &style);

        // Refresh installations
        let installations = crate::discovery::discover_installations();
        let count = installations.len();
        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            state.replace(installations);
        }

        cx.bus().publish_t(
            topics::INSTALLATIONS_REFRESHED,
            topics::InstallationsRefreshed,
        );

        // Auto-load bindings if enabled
        if self.auto_load && count > 0 {
            self.reload_bindings(cx);
        }

        if count > 0 {
            // Publish installation changed for all listeners
            self.publish_current_installation(cx);
            cx.sd().show_ok(ev.context);
        } else {
            cx.sd().show_alert(ev.context);
        }

        self.render_status(cx, ev.context);
    }

    fn on_global_event(&mut self, cx: &Context, ev: &IncomingEvent) {
        // Watch for global settings changes made by other Settings instances
        if let IncomingEvent::DidReceiveGlobalSettings { payload, .. } = ev {
            let (changed_folder, changed_style) = self.sync_from_global_map(&payload.settings);
            if changed_folder {
                self.handle_icon_folder_change(cx);
            }
            if changed_style {
                cx.bus()
                    .publish_t(topics::STYLE_CHANGED, topics::StyleChanged);
            }
        }
    }

    fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
        if req.event == "getStyles" {
            let mut items = vec![DataSourceResultItem::Item(DataSourceItem {
                disabled: None,
                label: Some("Default".to_string()),
                value: String::new(),
            })];

            if let Some(styles) = cx.try_ext::<StylesState>() {
                for (id, name) in styles.list() {
                    // Skip "default" — the empty-value first item already covers it
                    if id == "default" {
                        continue;
                    }
                    items.push(DataSourceResultItem::Item(DataSourceItem {
                        disabled: None,
                        label: Some(name),
                        value: id,
                    }));
                }
            }

            cx.sdpi().reply(req, items);
        }
    }

    fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
        if event.downcast(topics::BINDINGS_RELOAD_REQUESTED).is_some() {
            debug!("SettingsAction: file watcher requested binding reload");
            self.reload_bindings(cx);
            self.render_status(cx, ctx_id);
            return;
        }
        if event.downcast(topics::INSTALLATION_CHANGED).is_some()
            || event.downcast(topics::INSTALLATIONS_REFRESHED).is_some()
            || event.downcast(topics::STYLE_CHANGED).is_some()
        {
            self.render_status(cx, ctx_id);
        }
    }
}

// ── Private Implementation ──────────────────────────────────────────────────────

impl SettingsAction {
    fn apply_settings(&mut self, settings: &serde_json::Map<String, serde_json::Value>) {
        if let Some(v) = settings.get(KEY_AUTO_LOAD).and_then(|v| v.as_bool()) {
            self.auto_load = v;
        }
        if let Some(v) = settings.get(KEY_AUTO_SELECT).and_then(|v| v.as_bool()) {
            self.auto_select_last = v;
        }
        if let Some(v) = settings.get(KEY_ICON_FOLDER).and_then(|v| v.as_str()) {
            self.icon_folder = v.to_string();
        }
        if let Some(v) = settings.get(KEY_DEFAULT_STYLE).and_then(|v| v.as_str()) {
            self.default_style = v.to_string();
        }
    }

    fn hydrate_from_globals(&mut self, cx: &Context) {
        let globals = cx.globals();
        if let Some(v) = globals.get(KEY_AUTO_LOAD).and_then(|v| v.as_bool()) {
            self.auto_load = v;
        }
        if let Some(v) = globals.get(KEY_AUTO_SELECT).and_then(|v| v.as_bool()) {
            self.auto_select_last = v;
        }
        if let Some(v) = globals
            .get(KEY_ICON_FOLDER)
            .and_then(|v| v.as_str().map(String::from))
        {
            self.icon_folder = v;
        }
        if let Some(v) = globals
            .get(KEY_DEFAULT_STYLE)
            .and_then(|v| v.as_str().map(String::from))
        {
            self.default_style = v;
        }
    }

    fn promote_to_globals(&self, cx: &Context) {
        let globals = cx.globals();
        globals.with_mut(|map| {
            map.insert(
                KEY_AUTO_LOAD.into(),
                serde_json::Value::Bool(self.auto_load),
            );
            map.insert(
                KEY_AUTO_SELECT.into(),
                serde_json::Value::Bool(self.auto_select_last),
            );
            map.insert(
                KEY_ICON_FOLDER.into(),
                serde_json::Value::String(self.icon_folder.clone()),
            );
            map.insert(
                KEY_DEFAULT_STYLE.into(),
                serde_json::Value::String(self.default_style.clone()),
            );
        });
    }

    /// Sync local state from a global settings map.
    /// Returns `(icon_folder_changed, style_changed)`.
    fn sync_from_global_map(
        &mut self,
        settings: &serde_json::Map<String, serde_json::Value>,
    ) -> (bool, bool) {
        let old_folder = self.icon_folder.clone();
        let old_style = self.default_style.clone();
        self.apply_settings(settings);
        (
            self.icon_folder != old_folder,
            self.default_style != old_style,
        )
    }

    fn handle_icon_folder_change(&self, cx: &Context) {
        if let Some(icon_state) = cx.try_ext::<IconFolderState>() {
            if self.icon_folder.is_empty() {
                icon_state.clear();
            } else {
                icon_state.set(PathBuf::from(&self.icon_folder));
            }
        }

        cx.bus()
            .publish_t(topics::ICON_FOLDER_CHANGED, topics::IconFolderChanged);
    }

    fn reload_bindings(&self, cx: &Context) {
        crate::reload_bindings(cx);
    }

    fn publish_current_installation(&self, cx: &Context) {
        cx.bus()
            .publish_t(topics::INSTALLATION_CHANGED, topics::InstallationChanged);
    }

    /// Resolve the effective style for this action's rendering.
    ///
    /// The Settings action uses the global default style directly (since it
    /// controls the global setting, it should reflect whatever the user chose).
    fn resolve_style(&self, cx: &Context) -> KeyStyle {
        if let Some(styles) = cx.try_ext::<StylesState>() {
            // Use empty string for per-key — Settings action always uses the global default
            crate::state::styles::resolve_style("", &styles, &cx.globals())
        } else {
            crate::styles::style_default()
        }
    }

    fn render_status(&self, cx: &Context, ctx_id: &str) {
        let style = self.resolve_style(cx);
        let mut lines = Vec::new();

        // Installation info
        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            let snap = state.snapshot();
            if let Some(inst) = snap.current() {
                lines.push(inst.channel.display_name().to_string());
                lines.push(inst.short_version().to_string());
            } else {
                lines.push("No SC".to_string());
            }
        }

        // Binding info
        if let Some(state) = cx.try_ext::<BindingsState>() {
            let snap = state.snapshot();
            if let Some(ref bindings) = snap.bindings {
                lines.push(format!("{} binds", bindings.action_count()));
            }
        }

        let text = if lines.is_empty() {
            "Settings".to_string()
        } else {
            lines.join("\n")
        };

        render::render_multiline(cx, ctx_id, &text, &style);
    }
}
