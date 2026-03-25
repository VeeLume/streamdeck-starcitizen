use streamdeck_lib::prelude::*;

// ── Installation Changed ─────────────────────────────────────────────────────

/// Published when the active installation switches.
/// Actions should re-read from `ActiveInstallationState` for current data.
pub const INSTALLATION_CHANGED: TopicId<InstallationChanged> =
    TopicId::new("starcitizen.installation-changed");

#[derive(Debug, Clone)]
pub struct InstallationChanged;

// ── Bindings Reloaded ────────────────────────────────────────────────────────

/// Published after bindings are loaded (or fail to load).
/// Actions should re-read from `BindingsState` for current data.
pub const BINDINGS_RELOADED: TopicId<BindingsReloaded> =
    TopicId::new("starcitizen.bindings-reloaded");

#[derive(Debug, Clone)]
pub struct BindingsReloaded;

// ── Icon Folder Changed ──────────────────────────────────────────────────────

/// Published when the global icon folder setting changes.
/// Actions should re-read from `IconFolderState` for current path.
pub const ICON_FOLDER_CHANGED: TopicId<IconFolderChanged> =
    TopicId::new("starcitizen.icon-folder-changed");

#[derive(Debug, Clone)]
pub struct IconFolderChanged;

// ── Installations Refreshed ──────────────────────────────────────────────────

/// Published after a full re-scan of installations.
/// Actions should re-read from `ActiveInstallationState` for current data.
pub const INSTALLATIONS_REFRESHED: TopicId<InstallationsRefreshed> =
    TopicId::new("starcitizen.installations-refreshed");

#[derive(Debug, Clone)]
pub struct InstallationsRefreshed;

// ── Style Changed ───────────────────────────────────────────────────────────

/// Published when the global default style changes.
/// Actions should re-render to reflect the new style.
pub const STYLE_CHANGED: TopicId<StyleChanged> = TopicId::new("starcitizen.style-changed");

#[derive(Debug, Clone)]
pub struct StyleChanged;
