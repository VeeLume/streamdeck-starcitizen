use constcat::concat;
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::debug;

use crate::PLUGIN_ID;
use crate::discovery::Channel;
use crate::render;
use crate::state::installations::ActiveInstallationState;
use crate::state::styles::StylesState;
use crate::styles::KeyStyle;
use crate::topics;

// ── Mode ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Show,
    Pin,
    Cycle,
}

impl Mode {
    fn from_str(s: &str) -> Self {
        match s {
            "pin" => Self::Pin,
            "cycle" => Self::Cycle,
            _ => Self::Show,
        }
    }
}

// ── Action ──────────────────────────────────────────────────────────────────────

pub struct ManageVersionAction {
    mode: Mode,
    pinned_channel: Option<Channel>,
    key_style: String,
}

impl Default for ManageVersionAction {
    fn default() -> Self {
        Self {
            mode: Mode::Show,
            pinned_channel: None,
            key_style: String::new(),
        }
    }
}

impl ActionStatic for ManageVersionAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".manage-version");
}

impl Action for ManageVersionAction {
    fn id(&self) -> &str {
        Self::ID
    }

    fn topics(&self) -> &'static [&'static str] {
        &[
            topics::INSTALLATION_CHANGED.name,
            topics::INSTALLATIONS_REFRESHED.name,
            topics::STYLE_CHANGED.name,
        ]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) {
        debug!("ManageVersionAction will_appear: {}", ev.context);
        self.apply_settings(ev.settings);
        self.render(cx, ev.context);
    }

    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) {
        self.apply_settings(ev.settings);
        self.send_ui_state(cx, ev.context);
        self.render(cx, ev.context);
    }

    fn property_inspector_did_appear(&mut self, cx: &Context, ev: &PropertyInspectorDidAppear) {
        self.send_ui_state(cx, ev.context);
    }

    fn did_receive_property_inspector_message(
        &mut self,
        cx: &Context,
        ev: &DidReceivePropertyInspectorMessage,
        _is_sdpi: bool,
    ) {
        if let Some(action) = ev.payload.get("action").and_then(|v| v.as_str())
            && action == "getUiState"
        {
            self.send_ui_state(cx, ev.context);
        }
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        debug!("ManageVersionAction key_down mode={:?}", self.mode);

        match self.mode {
            Mode::Show => {
                // Refresh installations
                self.refresh_installations(cx, ev.context);
            }
            Mode::Pin => {
                // Switch to the pinned channel
                if let Some(channel) = self.pinned_channel
                    && let Some(state) = cx.try_ext::<ActiveInstallationState>()
                {
                    if state.select_by_channel(channel) {
                        self.publish_installation_changed(cx);
                    } else {
                        cx.sd().show_alert(ev.context);
                    }
                }
            }
            Mode::Cycle => {
                // Advance to next installation
                if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
                    state.next();
                    self.publish_installation_changed(cx);
                }
            }
        }
    }

    fn dial_rotate(&mut self, cx: &Context, ev: &DialRotate) {
        // Only meaningful in Cycle mode — rotate through installations
        if self.mode != Mode::Cycle {
            return;
        }
        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            if *ev.ticks > 0 {
                state.next();
            } else {
                state.previous();
            }
            self.publish_installation_changed(cx);
        }
    }

    fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
        match req.event {
            "getChannels" => {
                let items = [
                    Channel::Live,
                    Channel::Hotfix,
                    Channel::Ptu,
                    Channel::Eptu,
                    Channel::TechPreview,
                ]
                .iter()
                .map(|ch| {
                    DataSourceResultItem::Item(DataSourceItem {
                        disabled: None,
                        label: Some(ch.display_name().to_string()),
                        value: ch.display_name().to_string(),
                    })
                })
                .collect::<Vec<_>>();

                cx.sdpi().reply(req, items);
            }
            "getStyles" => {
                reply_styles(cx, req);
            }
            _ => {}
        }
    }

    fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
        if event.downcast(topics::INSTALLATION_CHANGED).is_some()
            || event.downcast(topics::INSTALLATIONS_REFRESHED).is_some()
            || event.downcast(topics::STYLE_CHANGED).is_some()
        {
            self.render(cx, ctx_id);
        }
    }
}

impl ManageVersionAction {
    fn send_ui_state(&self, cx: &Context, ctx_id: &str) {
        let mode_str = match self.mode {
            Mode::Show => "show",
            Mode::Pin => "pin",
            Mode::Cycle => "cycle",
        };
        cx.sd().send_to_property_inspector(
            ctx_id,
            serde_json::json!({
                "type": "uiState",
                "mode": mode_str,
            }),
        );
    }

    fn apply_settings(&mut self, settings: &serde_json::Map<String, serde_json::Value>) {
        if let Some(mode_val) = settings.get("mode").and_then(|v| v.as_str()) {
            self.mode = Mode::from_str(mode_val);
        }
        if let Some(ch_val) = settings.get("pinnedChannel").and_then(|v| v.as_str()) {
            self.pinned_channel = Channel::from_str_loose(ch_val);
        }
        if let Some(v) = settings.get("keyStyle").and_then(|v| v.as_str()) {
            self.key_style = v.to_string();
        }
    }

    fn resolve_style(&self, cx: &Context) -> KeyStyle {
        if let Some(styles) = cx.try_ext::<StylesState>() {
            crate::state::styles::resolve_style(&self.key_style, &styles, &cx.globals())
        } else {
            crate::styles::style_default()
        }
    }

    fn render(&self, cx: &Context, ctx_id: &str) {
        let style = self.resolve_style(cx);
        let state = cx.try_ext::<ActiveInstallationState>();
        let snap = state.as_ref().map(|s| s.snapshot());
        let current = snap.as_ref().and_then(|s| s.current());

        match self.mode {
            Mode::Show => {
                if let Some(inst) = current {
                    render::render_channel(
                        cx,
                        ctx_id,
                        inst.channel.display_name(),
                        inst.short_version(),
                        &style,
                    );
                } else {
                    render::render_progress(cx, ctx_id, "No SC\nfound", &style);
                }
            }
            Mode::Pin => {
                let channel_name = self
                    .pinned_channel
                    .map(|c| c.display_name())
                    .unwrap_or("---");
                let is_active = match (self.pinned_channel, current) {
                    (Some(pinned), Some(inst)) => inst.channel == pinned,
                    _ => false,
                };
                render::render_channel_pin(cx, ctx_id, channel_name, is_active, &style);
            }
            Mode::Cycle => {
                if let Some(inst) = current {
                    let next = snap
                        .as_ref()
                        .and_then(|s| s.next_channel())
                        .unwrap_or("---");
                    render::render_channel_cycle(
                        cx,
                        ctx_id,
                        inst.channel.display_name(),
                        inst.short_version(),
                        next,
                        &style,
                    );
                } else {
                    render::render_progress(cx, ctx_id, "No SC\nfound", &style);
                }
            }
        }
    }

    fn refresh_installations(&self, cx: &Context, ctx_id: &str) {
        let style = self.resolve_style(cx);
        render::render_progress(cx, ctx_id, "Scanning\u{2026}", &style);

        let installations = crate::discovery::discover_installations();
        let count = installations.len();

        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            state.replace(installations);
        }

        cx.bus().publish_t(
            topics::INSTALLATIONS_REFRESHED,
            topics::InstallationsRefreshed,
        );

        if count > 0 {
            self.publish_installation_changed(cx);
            cx.sd().show_ok(ctx_id);
        } else {
            cx.sd().show_alert(ctx_id);
        }
    }

    fn publish_installation_changed(&self, cx: &Context) {
        cx.bus()
            .publish_t(topics::INSTALLATION_CHANGED, topics::InstallationChanged);
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
