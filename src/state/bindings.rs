use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::bindings::model::ParsedBindings;
use crate::bindings::overlay::UserOverride;
use crate::discovery::Channel;

/// Shared data for loaded keybindings.
#[derive(Debug, Clone, Default)]
pub struct BindingsData {
    pub bindings: Option<ParsedBindings>,
    /// Raw user overrides from actionmaps.xml, preserved so the generated
    /// profile can include them and avoid resetting user customisations.
    pub user_overrides: Vec<UserOverride>,
    #[allow(dead_code)] // Tracks which channel's bindings are loaded; useful for PI status
    pub channel: Option<Channel>,
    #[allow(dead_code)] // Stores load error message; useful for PI status display
    pub error: Option<String>,
}

/// Thread-safe store for the active bindings state.
pub struct BindingsState {
    inner: ArcSwap<BindingsData>,
}

impl BindingsState {
    pub fn new() -> Self {
        Self {
            inner: ArcSwap::from_pointee(BindingsData::default()),
        }
    }

    pub fn snapshot(&self) -> Arc<BindingsData> {
        self.inner.load_full()
    }

    pub fn replace(&self, data: BindingsData) {
        self.inner.store(Arc::new(data));
    }

    /// Run a function with read access to the parsed bindings, if loaded.
    pub fn with_bindings<R>(&self, f: impl FnOnce(&ParsedBindings) -> R) -> Option<R> {
        let snap = self.inner.load_full();
        snap.bindings.as_ref().map(f)
    }
}
