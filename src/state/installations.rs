use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::discovery::{Channel, Installation};

/// Shared data for all discovered installations.
#[derive(Debug, Clone, Default)]
pub struct InstallationData {
    pub installations: Vec<Installation>,
    pub selected_index: usize,
    pub last_launched: Option<Channel>,
}

impl InstallationData {
    /// The currently selected installation, if any.
    pub fn current(&self) -> Option<&Installation> {
        self.installations.get(self.selected_index)
    }

    /// The installation after the current one (wraps around).
    pub fn next_channel(&self) -> Option<&str> {
        if self.installations.len() <= 1 {
            return None;
        }
        let next = (self.selected_index + 1) % self.installations.len();
        Some(self.installations[next].channel.display_name())
    }
}

/// Thread-safe store for the active installation state.
///
/// Uses `ArcSwap` for lock-free reads with occasional full replacements.
pub struct ActiveInstallationState {
    inner: ArcSwap<InstallationData>,
}

impl ActiveInstallationState {
    pub fn new() -> Self {
        Self {
            inner: ArcSwap::from_pointee(InstallationData::default()),
        }
    }

    /// Get a snapshot of the current state (lock-free).
    pub fn snapshot(&self) -> Arc<InstallationData> {
        self.inner.load_full()
    }

    /// Replace the entire installation list. Auto-selects the highest-priority channel.
    pub fn replace(&self, installations: Vec<Installation>) {
        let mut data = InstallationData {
            installations,
            selected_index: 0,
            last_launched: self.inner.load().last_launched,
        };
        // Auto-select: installations are already sorted by priority from discovery
        data.selected_index = 0;
        self.inner.store(Arc::new(data));
    }

    /// Select an installation by channel. Returns true if found.
    pub fn select_by_channel(&self, channel: Channel) -> bool {
        let mut data = (*self.inner.load_full()).clone();
        if let Some(pos) = data.installations.iter().position(|i| i.channel == channel) {
            data.selected_index = pos;
            self.inner.store(Arc::new(data));
            true
        } else {
            false
        }
    }

    /// Advance to the next installation (wraps around).
    pub fn next(&self) {
        let mut data = (*self.inner.load_full()).clone();
        if !data.installations.is_empty() {
            data.selected_index = (data.selected_index + 1) % data.installations.len();
            self.inner.store(Arc::new(data));
        }
    }

    /// Go back to the previous installation (wraps around).
    pub fn previous(&self) {
        let mut data = (*self.inner.load_full()).clone();
        if !data.installations.is_empty() {
            data.selected_index = if data.selected_index == 0 {
                data.installations.len() - 1
            } else {
                data.selected_index - 1
            };
            self.inner.store(Arc::new(data));
        }
    }

    /// Record which channel was most recently launched.
    pub fn set_last_launched(&self, channel: Channel) {
        let mut data = (*self.inner.load_full()).clone();
        data.last_launched = Some(channel);
        self.inner.store(Arc::new(data));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_install(channel: Channel) -> Installation {
        Installation {
            channel,
            path: PathBuf::from(format!("C:\\SC\\{}", channel.display_name())),
            version: "4.6.1.0".to_string(),
            branch: "main".to_string(),
            build_id: "1".to_string(),
        }
    }

    #[test]
    fn cycling_through_installations() {
        let state = ActiveInstallationState::new();
        state.replace(vec![
            make_install(Channel::Live),
            make_install(Channel::Ptu),
            make_install(Channel::Eptu),
        ]);

        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Live);

        state.next();
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Ptu);

        state.next();
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Eptu);

        // Wrap around
        state.next();
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Live);

        // Previous wraps backward
        state.previous();
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Eptu);
    }

    #[test]
    fn select_by_channel() {
        let state = ActiveInstallationState::new();
        state.replace(vec![
            make_install(Channel::Live),
            make_install(Channel::Ptu),
        ]);

        assert!(state.select_by_channel(Channel::Ptu));
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Ptu);

        assert!(!state.select_by_channel(Channel::Eptu)); // not found
        assert_eq!(state.snapshot().current().unwrap().channel, Channel::Ptu); // unchanged
    }

    #[test]
    fn empty_state_is_safe() {
        let state = ActiveInstallationState::new();
        assert!(state.snapshot().current().is_none());
        state.next(); // should not panic
        state.previous(); // should not panic
    }
}
