use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crossbeam_channel::{Receiver, bounded, select};
use streamdeck_lib::prelude::*;
use tracing::{debug, info, warn};

use crate::state::bindings::{BindingsData, BindingsState};
use crate::state::installations::ActiveInstallationState;
use crate::topics;

/// Watches `actionmaps.xml` for changes by polling its modification time.
///
/// When the file changes (e.g. the player exits the SC keybinding settings),
/// reloads bindings directly and publishes `BINDINGS_RELOADED` so actions
/// can re-render. This runs independently of any visible action — it does not
/// require a Settings key on the deck.
///
/// Uses `OnAppLaunch` start policy (only polls while SC is running) with mtime
/// polling instead of OS-level file
/// watching. This avoids Windows `ReadDirectoryChangesW` reliability issues
/// and gracefully handles the file/directory not yet existing.
pub struct BindingWatcherAdapter;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Resolve the `actionmaps.xml` path from the current active installation.
fn resolve_overlay_path(state: &Option<Arc<ActiveInstallationState>>) -> Option<PathBuf> {
    state.as_ref().and_then(|s| {
        let snap = s.snapshot();
        snap.current()
            .map(|i| i.path.join("user/client/0/Profiles/default/actionmaps.xml"))
    })
}

/// Read the file's mtime, returning `None` if the file doesn't exist.
fn file_mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Load bindings and store the result. Returns `true` on success.
fn do_reload(
    install_state: &Option<Arc<ActiveInstallationState>>,
    bindings_state: &Option<Arc<BindingsState>>,
    bus: &Arc<dyn Bus>,
) -> bool {
    let install_path = install_state.as_ref().and_then(|state| {
        let snap = state.snapshot();
        snap.current().map(|i| i.path.clone())
    });

    let Some(path) = install_path else {
        warn!("BindingWatcher: no active installation to reload from");
        return false;
    };

    info!("BindingWatcher: reloading bindings from {}", path.display());
    match crate::bindings::load_bindings(&path) {
        Ok(loaded) => {
            let channel = install_state
                .as_ref()
                .and_then(|s| s.snapshot().current().map(|i| i.channel));

            let action_count = loaded.bindings.action_count();

            if let Some(state) = bindings_state {
                state.replace(BindingsData {
                    bindings: Some(loaded.bindings),
                    user_overrides: loaded.user_overrides,
                    channel,
                    error: None,
                });
            }

            bus.publish_t(topics::BINDINGS_RELOADED, topics::BindingsReloaded);
            info!("BindingWatcher: reloaded ({action_count} actions)");
            true
        }
        Err(e) => {
            let msg = format!("{e:#}");
            warn!("BindingWatcher: reload failed: {msg}");

            if let Some(state) = bindings_state {
                state.replace(BindingsData {
                    bindings: None,
                    user_overrides: Vec::new(),
                    channel: None,
                    error: Some(msg),
                });
            }

            bus.publish_t(topics::BINDINGS_RELOADED, topics::BindingsReloaded);
            false
        }
    }
}

impl Adapter for BindingWatcherAdapter {
    fn name(&self) -> &'static str {
        "starcitizen.binding-watcher"
    }

    fn policy(&self) -> StartPolicy {
        StartPolicy::OnAppLaunch
    }

    fn topics(&self) -> &'static [&'static str] {
        &[topics::INSTALLATION_CHANGED.name]
    }

    fn start(
        &self,
        cx: &Context,
        bus: Arc<dyn Bus>,
        inbox: Receiver<Arc<ErasedTopic>>,
    ) -> AdapterResult {
        let (stop_tx, stop_rx) = bounded::<()>(1);
        let install_state = cx.try_ext::<ActiveInstallationState>();
        let bindings_state = cx.try_ext::<BindingsState>();

        let join = std::thread::spawn(move || {
            let mut last_mtime: Option<SystemTime> = None;
            let mut current_path: Option<PathBuf> = resolve_overlay_path(&install_state);

            // Seed mtime from the current file (if it exists)
            if let Some(ref path) = current_path {
                last_mtime = file_mtime(path);
                if last_mtime.is_some() {
                    info!("BindingWatcher: watching {} (poll)", path.display());
                } else {
                    debug!(
                        "BindingWatcher: {} not yet present, will poll",
                        path.display()
                    );
                }
            } else {
                info!("BindingWatcher: no active installation yet, waiting for topic");
            }

            loop {
                select! {
                    recv(stop_rx) -> _ => break,
                    recv(inbox) -> msg => {
                        if let Ok(ev) = msg
                            && ev.downcast(topics::INSTALLATION_CHANGED).is_some()
                        {
                            let new_path = resolve_overlay_path(&install_state);
                            if new_path != current_path {
                                current_path = new_path;
                                // Reset mtime so we don't false-trigger on the new path
                                last_mtime = current_path.as_ref().and_then(file_mtime);
                                if let Some(ref path) = current_path {
                                    info!("BindingWatcher: now watching {}", path.display());
                                } else {
                                    info!("BindingWatcher: installation cleared");
                                }
                            }
                        }
                    },
                    default(POLL_INTERVAL) => {
                        let Some(ref path) = current_path else { continue };
                        let current_mtime = file_mtime(path);

                        // Detect change: file appeared or mtime advanced
                        let changed = match (last_mtime, current_mtime) {
                            (None, Some(_)) => true,
                            (Some(old), Some(new)) => new > old,
                            _ => false,
                        };

                        if changed {
                            do_reload(&install_state, &bindings_state, &bus);
                            // Re-read mtime AFTER reload completes to absorb any
                            // writes that happened during the ~3s reload window,
                            // preventing cascade reloads.
                            last_mtime = file_mtime(path);
                        }
                    }
                }
            }

            info!("BindingWatcher: stopped");
        });

        Ok(AdapterHandle::from_crossbeam(join, stop_tx))
    }
}
