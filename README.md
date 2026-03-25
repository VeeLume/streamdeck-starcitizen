# Star Citizen Tools — Stream Deck Plugin

A Stream Deck plugin for **Star Citizen** that lets you execute any in-game action from a physical button. For the hundreds of actions that lack a keyboard shortcut, the plugin can generate a keybinding profile to import into the game.

Built with Rust and [`streamdeck-lib`](https://github.com/veelume/streamdeck-lib).

## Features

- **Execute Action** — Pick any Star Citizen keybind from a dropdown and fire it with a button press. Supports hold-press and double-press for up to 3 actions per key.
- **Manage Version** — Display and switch between Star Citizen installations (LIVE, PTU, EPTU, Hotfix, TechPreview). Show, pin, or cycle modes.
- **Generate Binds** — Auto-generate conflict-free keyboard shortcuts for actions that have no keyboard binding by default.
- **Settings** — Configure auto-load, auto-select last launched channel, icon folder, and default key style.
- **Custom rendering** — Every button is rendered as a full PNG with custom fonts, abbreviations, and configurable styles (solid border or vignette glow).
- **Live reload** — File watcher detects when you change keybindings in-game and updates buttons automatically.

## Install (Users)

1. Download the latest `.streamDeckPlugin` from [Releases](../../releases).
2. Double-click — the Stream Deck app installs it automatically.
3. Find the **Star Citizen** category in the action list.

## Build (Developers)

**Prerequisites:** Rust (stable), [Elgato CLI](https://docs.elgato.com/streamdeck/sdk/introduction/getting-started) (`npm install -g @elgato/cli`).

```sh
git clone <repo-url>
cd streamdeck-starcitizen
cargo build --release
```

### Development Workflow

Use the **Build Plugin** VS Code task (`Ctrl+Shift+B`) which:
1. Stops the running plugin
2. Builds release binary
3. Copies exe into the sdPlugin bundle
4. Restarts the plugin

Or manually:
```sh
cargo build --release
cp target/release/plugin.exe icu.veelume.starcitizen.sdPlugin/bin/icu.veelume.starcitizen.exe
streamdeck restart icu.veelume.starcitizen
```

### Release

This project uses [Conventional Commits](https://www.conventionalcommits.org/) enforced by a
`commit-msg` git hook, and [git-cliff](https://git-cliff.org/) for automated changelogs.

```sh
# Install tools (one-time)
cargo install cargo-release git-cliff

# Create a release (bumps version, generates CHANGELOG.md, commits, tags, pushes)
cargo release patch   # or: minor, major
```

GitHub Actions then builds, packs, validates, and creates a GitHub Release with
auto-generated release notes grouped by commit type.

## Project Structure

```
├── src/
│   ├── main.rs                # Plugin entrypoint, hooks, registration
│   ├── actions/               # Stream Deck actions (one file per action)
│   │   ├── execute_action.rs  # Execute SC keybinds with hold/double-press
│   │   ├── manage_version.rs  # Display/switch SC installations
│   │   ├── settings.rs        # Plugin-wide configuration
│   │   └── generate_binds.rs  # Auto-generate missing keybindings
│   ├── adapters/
│   │   └── binding_watcher.rs # File watcher for live keybind reload
│   ├── state/                 # Shared state stores (Extensions)
│   │   ├── installations.rs   # Active installation tracking
│   │   ├── bindings.rs        # Parsed keybinding data
│   │   ├── fonts.rs           # Font registry (embedded + user fonts)
│   │   ├── styles.rs          # Key style registry (built-in + user styles)
│   │   └── icon_folder.rs     # User icon folder path
│   ├── bindings/              # Star Citizen keybinding subsystem
│   │   ├── parser.rs          # Parse defaultProfile.xml from Data.p4k
│   │   ├── overlay.rs         # Apply user actionmaps.xml customizations
│   │   ├── autofill.rs        # Generate missing keyboard shortcuts
│   │   ├── executor.rs        # Convert bindings to OS-level key simulation
│   │   ├── translations.rs    # Parse global.ini for UI labels
│   │   ├── model.rs           # Data structures for parsed bindings
│   │   └── p4k.rs             # Extract files from Data.p4k archives
│   ├── discovery.rs           # Detect SC installations from launcher log
│   ├── render.rs              # PNG button rendering with custom fonts
│   ├── styles.rs              # Key style definitions and presets
│   ├── icons.rs               # Fuzzy icon matching by action name
│   ├── abbreviations.rs       # Word abbreviation table for button labels
│   └── topics.rs              # Pub/sub topic definitions
├── icu.veelume.starcitizen.sdPlugin/
│   ├── manifest.json          # Elgato plugin manifest
│   ├── bin/                   # Compiled binary
│   ├── fonts/                 # UAV OSD Sans Mono (bundled font)
│   ├── images/                # Plugin/action/key icons
│   └── pi/                    # Property Inspector HTML (one per action)
├── docs/
│   └── star-citizen-domain.md # SC domain knowledge and glossary
├── .github/workflows/         # CI/CD (release with git-cliff notes)
├── .githooks/                 # pre-commit, commit-msg, pre-push
├── CHANGELOG.md               # Auto-generated changelog
├── CLAUDE.md                  # Guidelines for coding agents
└── README.md                  # This file
```

## Architecture

See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation, coding conventions, and agent guidelines.

See [docs/star-citizen-domain.md](docs/star-citizen-domain.md) for Star Citizen domain knowledge (keybinding system, installation discovery, translations).

## Acknowledgements

- [svarog](https://github.com/19h/svarog) by [@19h](https://github.com/19h) — Rust library for reading Star Citizen's `Data.p4k` archives and converting CryXML to standard XML
- [sdpi-components](https://github.com/GeekyEggo/sdpi-components) by [@GeekyEggo](https://github.com/GeekyEggo) — web-component library for building Stream Deck Property Inspector UIs

## Similar Projects

- [streamdeck-starcitizen](https://github.com/mhwlng/streamdeck-starcitizen) by mhwlng — the original Star Citizen Stream Deck plugin (C#), inspiration for this project
- [SCStreamDeck](https://github.com/Jarex985/SCStreamDeck/) by Jarex985

## License

MIT OR Apache-2.0
