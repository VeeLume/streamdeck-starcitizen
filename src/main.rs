mod actions;
mod adapters;
mod bindings;
mod discovery;
mod icons;
mod render;
mod state;
mod topics;

use std::path::PathBuf;
use std::sync::Arc;

use streamdeck_lib::input::InputAdapter;
use streamdeck_lib::prelude::*;
use tracing::{info, warn};

use actions::execute_action::ExecuteAction;
use actions::generate_binds::GenerateBindsAction;
use actions::manage_version::ManageVersionAction;
use actions::settings::SettingsAction;
use state::bindings::{BindingsData, BindingsState};
use state::icon_folder::IconFolderState;
use state::installations::ActiveInstallationState;

pub const PLUGIN_ID: &str = "icu.veelume.starcitizen";

fn main() -> anyhow::Result<()> {
    let _guard = init(PLUGIN_ID);

    info!("Starting Star Citizen Tools Stream Deck plugin");

    let hooks = AppHooks::new().append(|cx, ev| match ev {
        HookEvent::Init => {
            on_startup(cx);
        }
        HookEvent::ApplicationDidLaunch(app) if app.contains("StarCitizen") => {
            on_game_launch(cx, app);
        }
        HookEvent::ApplicationDidTerminate(app) if app.contains("StarCitizen") => {
            info!("StarCitizen terminated: {app}");
        }
        HookEvent::DidReceiveGlobalSettings(settings) => {
            on_global_settings_changed(cx, settings);
        }
        _ => {}
    });

    let plugin = Plugin::new()
        .set_hooks(hooks)
        .add_extension(Arc::new(ActiveInstallationState::new()))
        .add_extension(Arc::new(BindingsState::new()))
        .add_extension(Arc::new(IconFolderState::new()))
        .add_action(ActionFactory::default_of::<ManageVersionAction>())
        .add_action(ActionFactory::default_of::<ExecuteAction>())
        .add_action(ActionFactory::default_of::<SettingsAction>())
        .add_action(ActionFactory::default_of::<GenerateBindsAction>())
        .add_adapter(InputAdapter::new())
        .add_adapter(adapters::binding_watcher::BindingWatcherAdapter);

    run_plugin(plugin)
}

// ── Hook Handlers ────────────────────────────────────────────────────────────────

fn on_startup(cx: &Context) {
    // Discover installations eagerly so buttons render immediately
    let installations = discovery::discover_installations();
    let count = installations.len();
    info!("Startup discovery: found {count} installation(s)");

    if count > 0 {
        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            state.replace(installations);
        }

        cx.bus().publish_t(
            topics::INSTALLATIONS_REFRESHED,
            topics::InstallationsRefreshed,
        );

        // Publish the initial active installation
        if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
            if state.snapshot().current().is_some() {
                cx.bus()
                    .publish_t(topics::INSTALLATION_CHANGED, topics::InstallationChanged);
            }
        }

        // Auto-load bindings on startup (default: on)
        let auto_load = cx
            .globals()
            .get("autoLoadBindings")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if auto_load {
            reload_bindings(cx);
        }
    }
}

fn on_game_launch(cx: &Context, app: &str) {
    info!("StarCitizen launched: {app}");

    // Detect channel from the executable name
    let channel = discovery::detect_channel_from_app(app);

    // Record last launched channel
    if let (Some(ch), Some(state)) = (channel, cx.try_ext::<ActiveInstallationState>()) {
        state.set_last_launched(ch);
    }

    // Refresh installations
    let installations = discovery::discover_installations();
    let count = installations.len();

    if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
        state.replace(installations);

        // Auto-select last launched if enabled
        let auto_select = cx
            .globals()
            .get("autoSelectLastLaunched")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if auto_select && let Some(ch) = channel {
            state.select_by_channel(ch);
        }
    }

    cx.bus().publish_t(
        topics::INSTALLATIONS_REFRESHED,
        topics::InstallationsRefreshed,
    );

    // Publish installation changed
    if let Some(state) = cx.try_ext::<ActiveInstallationState>() {
        if state.snapshot().current().is_some() {
            cx.bus()
                .publish_t(topics::INSTALLATION_CHANGED, topics::InstallationChanged);
        }
    }

    // Auto-load bindings if enabled
    let auto_load = cx
        .globals()
        .get("autoLoadBindings")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if auto_load && count > 0 {
        reload_bindings(cx);
    }
}

fn on_global_settings_changed(cx: &Context, settings: &serde_json::Map<String, serde_json::Value>) {
    // Sync icon folder state from global settings
    let new_folder = settings
        .get("iconFolder")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if let Some(icon_state) = cx.try_ext::<IconFolderState>() {
        let current = icon_state.path();
        let current_str = current
            .as_ref()
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if current_str != new_folder {
            if new_folder.is_empty() {
                icon_state.clear();
            } else {
                icon_state.set(PathBuf::from(new_folder));
            }

            cx.bus()
                .publish_t(topics::ICON_FOLDER_CHANGED, topics::IconFolderChanged);
        }
    }
}

/// Reload bindings from the active installation.
pub fn reload_bindings(cx: &Context) {
    let install_path = cx.try_ext::<ActiveInstallationState>().and_then(|state| {
        let snap = state.snapshot();
        snap.current().map(|i| i.path.clone())
    });

    let Some(path) = install_path else {
        warn!("No active installation to load bindings from");
        return;
    };

    info!("Loading bindings from {}", path.display());
    match bindings::load_bindings(&path) {
        Ok(parsed) => {
            let channel = cx
                .try_ext::<ActiveInstallationState>()
                .and_then(|s| s.snapshot().current().map(|i| i.channel));

            let map_count = parsed.map_count();
            let action_count = parsed.action_count();

            if let Some(state) = cx.try_ext::<BindingsState>() {
                state.replace(BindingsData {
                    bindings: Some(parsed),
                    channel,
                    error: None,
                });
            }

            cx.bus()
                .publish_t(topics::BINDINGS_RELOADED, topics::BindingsReloaded);

            info!("Bindings loaded: {map_count} maps, {action_count} actions");
        }
        Err(e) => {
            let msg = format!("{e:#}");
            warn!("Failed to load bindings: {msg}");

            if let Some(state) = cx.try_ext::<BindingsState>() {
                state.replace(BindingsData {
                    bindings: None,
                    channel: None,
                    error: Some(msg),
                });
            }

            cx.bus()
                .publish_t(topics::BINDINGS_RELOADED, topics::BindingsReloaded);
        }
    }
}
