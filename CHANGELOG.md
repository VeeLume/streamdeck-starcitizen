# Changelog

All notable changes to this project will be documented in this file.

## [0.1.1] - 2026-03-08

### Bug Fixes

- Reorder streamdeck-starcitizen package entry in Cargo.lock
- **adapter:** React to installation changes instead of one-shot resolve

The binding watcher previously resolved the installation path once at
    start time, idling forever if the state wasn't populated yet. Subscribe
    to INSTALLATION_CHANGED so the watcher sets up lazily when the install
    becomes available, and re-targets when the active installation switches.

### Documentation

- Add plugin specification

Describes the Star Citizen domain model, installation discovery,
    keybinding system, and all four planned Stream Deck actions.

### Features

- Add plugin assets, icons, fonts, and property inspector pages

Add action icons (SVG), plugin icon (PNG), starting-state key icons,
    UAV OSD Sans Mono font, and PI HTML pages for all four actions.
    Remove example placeholder assets.
- Add state stores and pub/sub topics

Add three shared state stores:
    - ActiveInstallationState: tracks discovered SC installations
    - BindingsState: holds parsed keybinding data
    - IconFolderState: tracks custom icon folder path

    Define five typed topics for decoupled communication between
    actions, adapters, and the plugin entrypoint.
- Add installation discovery and keybinding system

Add discovery module that scans RSI Launcher logs and filesystem
    to locate Star Citizen installations across all channels.

    Add bindings module with:
    - XML parser for actionmaps.xml keybinding files
    - Executor for sending keystrokes via Windows SendInput
    - Autofill for generating missing keyboard bindings
    - Overlay for merging default and user bindings
    - Translation layer for human-readable action names
    - P4K archive extraction support
- Add key rendering, icon helpers, and binding watcher adapter

Add render module for dynamic key icon generation using
    streamdeck-render (custom fonts, colored backgrounds, text layout).

    Add icons module for loading SVG/PNG from the icon folder.

    Add BindingWatcherAdapter that monitors actionmaps.xml for changes
    using the notify crate and triggers automatic binding reloads.
- Add actions and wire up plugin entrypoint

Add four Stream Deck actions:
    - ManageVersion: display/switch active SC installation
    - ExecuteAction: send keybinds via Windows SendInput
    - Settings: plugin-wide configuration and installation refresh
    - GenerateBinds: generate missing keyboard bindings to XML

    Update main.rs with full plugin wiring: hooks for startup,
    game launch/terminate, and global settings changes.

    Add extract-p4k utility binary for offline P4K archive extraction.

    Update manifest.json with real action definitions, application
    monitoring, and proper plugin metadata.

    Update Cargo.toml with all required dependencies.
- Add style system, custom fonts, and abbreviation table

Add a complete visual style system for key icon rendering:

    - KeyStyle type with colors, border, font sizing, line height, text
      transform, and abbreviation settings (src/styles.rs)
    - Built-in presets: Default (navy bg, blue accent) and SCK3 (transparent
      bg, white border, uppercase abbreviated labels)
    - User-defined custom styles via JSON files in %APPDATA%/styles/
    - StylesState registry with per-key → global → default cascade
    - STYLE_CHANGED topic for live re-rendering when global style changes

    Add custom font loading:
    - FontsState scans %APPDATA%/fonts/ for .ttf/.otf files
    - Per-key font selector in ExecuteAction PI
    - Style-level font field for style-wide font selection

    Add abbreviation table (src/abbreviations.rs):
    - ~60 Star Citizen-specific word-level abbreviations
    - Applied only to auto-derived titles, not custom user titles

    Update all render functions to accept &KeyStyle and use style colors/font
    instead of hardcoded constants. All four actions (ExecuteAction,
    ManageVersionAction, GenerateBindsAction, SettingsAction) resolve and
    pass styles. Style selector dropdowns added to all Property Inspectors.

    Fix text rendering:
    - Continuous font auto-scaling (28pt→10pt) prevents clipping
    - Width overflow check catches words wider than max_width
    - Literal \n from PI text fields rendered as actual line breaks
    - Configurable line_height multiplier with centered text block layout

### Miscellaneous

- Initial project scaffold
- Update build tooling and docs

- Add /p4k-extracted to .gitignore
    - Fix exe name in copy task (plugin.exe), add Extract P4K task
    - Remove profiling section from CLAUDE.md (covered by memory file)
- Remove accidentally committed __pycache__ and add to .gitignore

### Refactor

- Extract release task into standalone script

Move inline PowerShell from tasks.json into scripts/release.ps1 to fix
    quoting issues that caused the VS Code task to fail.

