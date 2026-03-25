use constcat::concat;
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;
use crate::bindings::autofill::{generate_bindings, render_xml};
use crate::bindings::generator_config;
use crate::render;
use crate::state::bindings::BindingsState;
use crate::state::installations::ActiveInstallationState;
use crate::state::styles::StylesState;
use crate::styles::KeyStyle;
use crate::topics;

// ── Action ──────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct GenerateBindsAction {
    profile_name: String,
    output_path_override: String,
    ignore_user_binds: bool,
    last_generated_count: Option<usize>,
    last_skipped_count: Option<usize>,
    key_style: String,
}

impl ActionStatic for GenerateBindsAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".generate-binds");
}

impl Action for GenerateBindsAction {
    fn id(&self) -> &str {
        Self::ID
    }

    fn topics(&self) -> &'static [&'static str] {
        &[
            topics::BINDINGS_RELOADED.name,
            topics::INSTALLATION_CHANGED.name,
            topics::STYLE_CHANGED.name,
        ]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) {
        debug!("GenerateBindsAction will_appear: {}", ev.context);
        self.apply_settings(ev.settings);
        self.render_status(cx, ev.context);
    }

    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) {
        self.apply_settings(ev.settings);
        self.render_status(cx, ev.context);
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        debug!("GenerateBindsAction key_down");
        let style = self.resolve_style(cx);
        render::render_progress(cx, ev.context, "Generating\u{2026}", &style);

        // Load generator config from TOML (or defaults if file missing)
        let mut config = match generator_config::load_config() {
            Ok(c) => c,
            Err(e) => {
                warn!("Generator config error: {e}");
                render::render_progress(cx, ev.context, "Config error", &style);
                cx.sd().show_alert(ev.context);
                return;
            }
        };
        config.profile_name = self.effective_profile_name();

        // Resolve bindings: either from state or fresh defaults-only
        let (bindings_owned, user_overrides);
        if self.ignore_user_binds {
            // Re-read Data.p4k for clean defaults (no user overlay)
            let Some(install_path) = self.resolve_install_path(cx) else {
                warn!("No active installation");
                render::render_progress(cx, ev.context, "No install", &style);
                cx.sd().show_alert(ev.context);
                return;
            };
            match crate::bindings::load_bindings_defaults_only(&install_path) {
                Ok(b) => {
                    bindings_owned = b;
                    user_overrides = Vec::new();
                }
                Err(e) => {
                    warn!("Failed to load default bindings: {e}");
                    render::render_progress(cx, ev.context, "Load error", &style);
                    cx.sd().show_alert(ev.context);
                    return;
                }
            }
        } else {
            // Use cached bindings from state
            let Some(bindings_state) = cx.try_ext::<BindingsState>() else {
                warn!("BindingsState not available");
                cx.sd().show_alert(ev.context);
                return;
            };
            let snap = bindings_state.snapshot();
            let Some(ref bindings) = snap.bindings else {
                warn!("No bindings loaded — load bindings first");
                render::render_progress(cx, ev.context, "No bindings", &style);
                cx.sd().show_alert(ev.context);
                return;
            };
            bindings_owned = bindings.clone();
            user_overrides = snap.user_overrides.clone();
        }

        // Generate bindings
        let result = generate_bindings(&bindings_owned, &config);
        let count = result.generated.len();
        let skipped = result.skipped.len();

        if count == 0 {
            info!("No unbound actions found — nothing to generate");
            self.last_generated_count = Some(0);
            render::render_progress(cx, ev.context, "0 binds\nAll bound!", &style);
            cx.sd().show_ok(ev.context);
            self.render_status(cx, ev.context);
            return;
        }

        if skipped > 0 {
            warn!("{skipped} actions could not be assigned a key combo (ran out of slots)");
            for s in &result.skipped {
                warn!(
                    action_map = s.action_map.as_str(),
                    action = s.action_name.as_str(),
                    "Skipped"
                );
            }
        }

        // Render XML (include user overrides only when not ignoring them)
        let xml = render_xml(&result.generated, &user_overrides, &config.profile_name);

        // Resolve output path
        let output_dir = if !self.output_path_override.is_empty() {
            Some(std::path::PathBuf::from(&self.output_path_override))
        } else {
            self.resolve_default_output_dir(cx)
        };

        let Some(output_dir) = output_dir else {
            warn!("Cannot determine output path — no active installation");
            render::render_progress(cx, ev.context, "No install", &style);
            cx.sd().show_alert(ev.context);
            return;
        };

        // Ensure directory exists
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            warn!("Failed to create output directory: {e}");
            render::render_progress(cx, ev.context, "Dir error", &style);
            cx.sd().show_alert(ev.context);
            return;
        }

        let filename = format!("{}.xml", config.profile_name);
        let output_path = output_dir.join(&filename);

        // Write file
        match std::fs::write(&output_path, &xml) {
            Ok(()) => {
                info!("Generated {count} bindings → {}", output_path.display());
                self.last_generated_count = Some(count);
                self.last_skipped_count = Some(skipped);
                if skipped > 0 {
                    cx.sd().show_alert(ev.context);
                } else {
                    cx.sd().show_ok(ev.context);
                }
            }
            Err(e) => {
                warn!("Failed to write {}: {e}", output_path.display());
                render::render_progress(cx, ev.context, "Write error", &style);
                cx.sd().show_alert(ev.context);
            }
        }

        self.render_status(cx, ev.context);
    }

    fn property_inspector_did_appear(&mut self, cx: &Context, ev: &PropertyInspectorDidAppear) {
        self.send_config_status(cx, ev.context);
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

        let action = ev
            .payload
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match action {
            "openGeneratorConfig" => {
                if let Err(e) = generator_config::open_config() {
                    warn!("Failed to open generator config: {e}");
                }
                self.send_config_status(cx, ev.context);
            }
            "resetGeneratorConfig" => {
                if let Err(e) = generator_config::reset_config() {
                    warn!("Failed to reset generator config: {e}");
                }
                self.send_config_status(cx, ev.context);
            }
            "getConfigStatus" => {
                self.send_config_status(cx, ev.context);
            }
            _ => {}
        }
    }

    fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
        if req.event == "getStyles" {
            reply_styles(cx, req);
        }
    }

    fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
        if event.downcast(topics::BINDINGS_RELOADED).is_some()
            || event.downcast(topics::INSTALLATION_CHANGED).is_some()
        {
            // Reset generation count when bindings or installation changes
            self.last_generated_count = None;
            self.last_skipped_count = None;
            self.render_status(cx, ctx_id);
        } else if event.downcast(topics::STYLE_CHANGED).is_some() {
            self.render_status(cx, ctx_id);
        }
    }
}

// ── Private Implementation ──────────────────────────────────────────────────────

impl GenerateBindsAction {
    fn apply_settings(&mut self, settings: &serde_json::Map<String, serde_json::Value>) {
        if let Some(v) = settings.get("profileName").and_then(|v| v.as_str()) {
            self.profile_name = v.to_string();
        }
        if let Some(v) = settings.get("outputPath").and_then(|v| v.as_str()) {
            self.output_path_override = v.to_string();
        }
        if let Some(v) = settings.get("ignoreUserBinds").and_then(|v| v.as_bool()) {
            self.ignore_user_binds = v;
        }
        if let Some(v) = settings.get("keyStyle").and_then(|v| v.as_str()) {
            self.key_style = v.to_string();
        }
    }

    fn effective_profile_name(&self) -> String {
        if self.profile_name.is_empty() {
            "icu-veelume-starcitizen".to_string()
        } else {
            self.profile_name.clone()
        }
    }

    fn resolve_install_path(&self, cx: &Context) -> Option<std::path::PathBuf> {
        let state = cx.try_ext::<ActiveInstallationState>()?;
        let snap = state.snapshot();
        let inst = snap.current()?;
        Some(inst.path.clone())
    }

    fn resolve_default_output_dir(&self, cx: &Context) -> Option<std::path::PathBuf> {
        self.resolve_install_path(cx).map(|p| {
            p.join("user")
                .join("client")
                .join("0")
                .join("controls")
                .join("mappings")
        })
    }

    fn send_config_status(&self, cx: &Context, ctx_id: &str) {
        let error = match generator_config::validate_config() {
            Ok(()) => serde_json::Value::Null,
            Err(msg) => serde_json::Value::String(msg),
        };
        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "configStatus",
                "error": error,
            }),
        );
    }

    fn resolve_style(&self, cx: &Context) -> KeyStyle {
        if let Some(styles) = cx.try_ext::<StylesState>() {
            crate::state::styles::resolve_style(&self.key_style, &styles, &cx.globals())
        } else {
            crate::styles::style_default()
        }
    }

    fn render_status(&self, cx: &Context, ctx_id: &str) {
        let style = self.resolve_style(cx);
        let mut lines = Vec::new();

        // Show binding count if available
        if let Some(bindings_state) = cx.try_ext::<BindingsState>() {
            let snap = bindings_state.snapshot();
            if snap.bindings.is_some() {
                if let Some(count) = self.last_generated_count {
                    if let Some(skipped) = self.last_skipped_count.filter(|&s| s > 0) {
                        lines.push(format!("{count} binds\n{skipped} skipped!"));
                    } else {
                        lines.push(format!("{count} binds"));
                    }
                } else {
                    lines.push("Ready".to_string());
                }
            } else {
                lines.push("No data".to_string());
            }
        }

        // Show profile name
        let profile = self.effective_profile_name();
        if profile != "icu-veelume-starcitizen" {
            lines.push(profile);
        }

        let text = if lines.is_empty() {
            "Generate".to_string()
        } else {
            lines.join("\n")
        };

        render::render_multiline(cx, ctx_id, &text, &style);
    }
}

/// Reply to a `getStyles` datasource request.
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
