use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, bounded, select};
use notify::{EventKind, RecursiveMode, Watcher};
use streamdeck_lib::prelude::*;
use tracing::{info, warn};

use crate::state::installations::ActiveInstallationState;
use crate::topics;

/// Watches `actionmaps.xml` for changes while Star Citizen is running.
///
/// When the file changes (e.g. the player exits the SC keybinding settings),
/// publishes `BINDINGS_RELOAD_REQUESTED` so that an action handler can trigger
/// a full binding reload.
pub struct BindingWatcherAdapter;

impl Adapter for BindingWatcherAdapter {
    fn name(&self) -> &'static str {
        "starcitizen.binding-watcher"
    }

    fn policy(&self) -> StartPolicy {
        StartPolicy::OnAppLaunch
    }

    fn topics(&self) -> &'static [&'static str] {
        &[]
    }

    fn start(
        &self,
        cx: &Context,
        bus: Arc<dyn Bus>,
        _inbox: Receiver<Arc<ErasedTopic>>,
    ) -> AdapterResult {
        let (stop_tx, stop_rx) = bounded::<()>(1);

        // Resolve overlay path from current installation
        let overlay_path = cx.try_ext::<ActiveInstallationState>().and_then(|s| {
            let snap = s.snapshot();
            snap.current()
                .map(|i| i.path.join("user/client/0/Profiles/default/actionmaps.xml"))
        });

        let Some(overlay_path) = overlay_path else {
            warn!("BindingWatcher: no active installation, adapter will idle");
            let join = std::thread::spawn(move || {
                let _ = stop_rx.recv();
            });
            return Ok(AdapterHandle::from_crossbeam(join, stop_tx));
        };

        let watch_dir = overlay_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| overlay_path.clone());

        let target_filename = overlay_path.file_name().map(|f| f.to_os_string());

        // Crossbeam channel bridging notify's callback to our select loop
        let (file_tx, file_rx) = bounded::<()>(8);

        let join = std::thread::spawn(move || {
            let target = target_filename;
            let tx = file_tx;

            let watcher_result =
                notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res
                        && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                    {
                        let is_target = event
                            .paths
                            .iter()
                            .any(|p| p.file_name().map(|f| f.to_os_string()) == target);
                        if is_target {
                            let _ = tx.send(());
                        }
                    }
                });

            let mut watcher = match watcher_result {
                Ok(w) => w,
                Err(e) => {
                    warn!("BindingWatcher: failed to create watcher: {e}");
                    let _ = stop_rx.recv();
                    return;
                }
            };

            if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
                warn!(
                    "BindingWatcher: failed to watch {}: {e}",
                    watch_dir.display()
                );
                let _ = stop_rx.recv();
                return;
            }

            info!("BindingWatcher: watching {}", overlay_path.display());

            let debounce = Duration::from_secs(1);
            let mut last_reload = Instant::now() - debounce; // allow immediate first trigger

            loop {
                select! {
                    recv(stop_rx) -> _ => break,
                    recv(file_rx) -> _ => {
                        if last_reload.elapsed() >= debounce {
                            info!("BindingWatcher: actionmaps.xml changed, requesting reload");
                            bus.publish_t(
                                topics::BINDINGS_RELOAD_REQUESTED,
                                topics::BindingsReloadRequested,
                            );
                            last_reload = Instant::now();
                        }
                    }
                }
            }

            info!("BindingWatcher: stopped");
        });

        Ok(AdapterHandle::from_crossbeam(join, stop_tx))
    }
}
