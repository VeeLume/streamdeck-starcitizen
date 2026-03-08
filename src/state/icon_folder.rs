use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;

/// Thread-safe store for the user-configured icon folder path.
pub struct IconFolderState {
    inner: ArcSwap<Option<PathBuf>>,
}

impl IconFolderState {
    pub fn new() -> Self {
        Self {
            inner: ArcSwap::from_pointee(None),
        }
    }

    /// Get the current icon folder path, if set.
    pub fn path(&self) -> Arc<Option<PathBuf>> {
        self.inner.load_full()
    }

    /// Set the icon folder path.
    pub fn set(&self, path: PathBuf) {
        self.inner.store(Arc::new(Some(path)));
    }

    /// Clear the icon folder path.
    pub fn clear(&self) {
        self.inner.store(Arc::new(None));
    }
}
