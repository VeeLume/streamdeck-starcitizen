use std::path::PathBuf;
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
///
/// Subscribes to `INSTALLATION_CHANGED` so the watcher can start (or move)
/// even if the active installation isn't known at adapter startup.
pub struct BindingWatcherAdapter;

/// Resolve the `actionmaps.xml` path from the current active installation.
fn resolve_overlay_path(state: &Option<Arc<ActiveInstallationState>>) -> Option<PathBuf> {
    state.as_ref().and_then(|s| {
        let snap = s.snapshot();
        snap.current()
            .map(|i| i.path.join("user/client/0/Profiles/default/actionmaps.xml"))
    })
}

/// Create a file-system watcher for the directory containing `overlay_path`,
/// filtering events to only the target filename. Returns `None` on failure.
fn create_watcher(
    overlay_path: &PathBuf,
    file_tx: crossbeam_channel::Sender<()>,
) -> Option<notify::RecommendedWatcher> {
    let watch_dir = overlay_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| overlay_path.clone());

    let target_filename = overlay_path.file_name().map(|f| f.to_os_string());

    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res
                && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
            {
                let is_target = event
                    .paths
                    .iter()
                    .any(|p| p.file_name().map(|f| f.to_os_string()) == target_filename);
                if is_target {
                    let _ = file_tx.send(());
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                warn!("BindingWatcher: failed to create watcher: {e}");
                return None;
            }
        };

    if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
        warn!(
            "BindingWatcher: failed to watch {}: {e}",
            watch_dir.display()
        );
        return None;
    }

    info!("BindingWatcher: watching {}", overlay_path.display());
    Some(watcher)
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

        let join = std::thread::spawn(move || {
            let debounce = Duration::from_secs(1);
            let mut last_reload = Instant::now() - debounce;

            // Channel bridging notify's callback into our select loop.
            // Cloned into each new watcher; all clones feed the same receiver.
            let (file_tx, file_rx) = bounded::<()>(8);

            // Try to set up the watcher immediately (installation may already be known)
            let mut active_watcher = resolve_overlay_path(&install_state)
                .and_then(|p| create_watcher(&p, file_tx.clone()));

            if active_watcher.is_none() {
                info!("BindingWatcher: no active installation yet, waiting for topic");
            }

            loop {
                select! {
                    recv(stop_rx) -> _ => break,
                    recv(inbox) -> msg => {
                        if let Ok(ev) = msg {
                            if ev.downcast(topics::INSTALLATION_CHANGED).is_some() {
                                // Drop old watcher (stops the watch), set up for new path
                                active_watcher = resolve_overlay_path(&install_state)
                                    .and_then(|p| create_watcher(&p, file_tx.clone()));
                                if active_watcher.is_none() {
                                    info!("BindingWatcher: installation cleared, watcher removed");
                                }
                            }
                        }
                    },
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
