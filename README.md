# Star Citizen Stream Deck

A Stream Deck plugin for **Star Citizen**.

[![Latest release](https://img.shields.io/github/v/release/VeeLume/streamdeck-starcitizen?display_name=tag)](https://github.com/VeeLume/streamdeck-starcitizen/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/VeeLume/streamdeck-starcitizen/total)](https://github.com/VeeLume/streamdeck-starcitizen/releases)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

Star Citizen has hundreds of in-game actions, and many of them ship without a keyboard shortcut at all — you have to open the options menu, find the entry, and bind a key yourself. **This plugin puts any of those actions on a Stream Deck button.** It reads the action list out of the `Data.p4k` shipped with your current install and overlays *your* `actionmaps.xml` on top, so the dropdown in the Property Inspector matches the build you're on and the rebinds you've made. For actions that have no keyboard binding, it can generate a conflict-free profile to import.

Built with Rust and [`streamdeck-lib`](https://github.com/veelume/streamdeck-lib).

> [!NOTE]
> Inspired by the original [streamdeck-starcitizen](https://github.com/mhwlng/streamdeck-starcitizen) plugin by mhwlng. That project inspired the idea and shaped a lot of how this one approaches the problem; this is a from-scratch Rust rewrite with live keybind reload, per-channel installation tracking, and full-PNG rendered buttons.

## What it does

- **Execute Action** — Pick any Star Citizen keybind from a searchable dropdown and fire it with a button press.
  - Supports **hold-press** and **double-press** for up to 3 actions per key.
  - Reads your `defaultProfile.xml` and overlays your `actionmaps.xml` so what you see is what's actually bound in-game.
- **Generate Binds** — Auto-fills the actions that ship without a keyboard shortcut.
  - Computes a conflict-free shortcut for every unbound action and exports a profile you can import in-game.
- **Manage Version** — Show, pin, or cycle between Star Citizen installations (LIVE, PTU, EPTU, Hotfix, TECH-PREVIEW).
  - Detected automatically from the RSI Launcher log.
- **Custom-rendered buttons** — Every key is a full PNG rendered with the bundled OSD font, automatic text wrapping, abbreviation table, and configurable styles (solid border or vignette glow). Drop your own `.ttf`/`.otf` into `%APPDATA%/icu.veelume.starcitizen/fonts/` and they're loaded at startup.
- **Live reload** — A file watcher catches changes to your keybindings in-game and refreshes every button automatically — no restart, no re-pick.
- **Plugin-wide settings** — Auto-load on launch, auto-select last-launched channel, icon folder, and default key style.

> [!TIP]
> Don't see your bindings? Launch the channel you want once from the RSI Launcher — installations are discovered by parsing the launcher's log, so each channel only appears after you've actually started it at least once.

> [!WARNING]
> Most Star Citizen patches just tweak default binds or add new actions, but every so often an action gets renamed, moved, or removed. If a button stops working after an update, open its Property Inspector and re-pick the action from the dropdown.

---

## Quick start

1. **Download** the latest `.streamDeckPlugin` from the [Releases page](https://github.com/VeeLume/streamdeck-starcitizen/releases/latest).
2. **Double-click it.** The Stream Deck app installs it automatically.
3. **Find the *Star Citizen* category** in the action list on the right side of the Stream Deck UI and drag *Execute Action* onto a key.
4. In the Property Inspector, **pick an action** from the dropdown. That's it.

To execute actions for an installation, just launch that channel once from the RSI Launcher so the plugin can discover it.

---

## Known limitations

- **Run Stream Deck as administrator if button presses don't register.** When Star Citizen runs elevated but Stream Deck does not, Windows blocks input injection into the game (UIPI). Right-click the Stream Deck desktop app and choose *Run as administrator*, then relaunch.
- **Mouse-bound actions may not register while aiming.** EasyAntiCheat blocks synthetic mouse-button events when Star Citizen is in relative-mouse mode (actively aiming, mouse-look, etc.). Prefer keyboard bindings for actions you need during combat — keyboard input is unaffected.
- **Keyboard-only execution.** Joystick and gamepad bindings are parsed but can't be fired from a Stream Deck button — only keyboard and mouse inputs can be simulated. Use *Generate Binds* to add keyboard shortcuts to actions that only have analog defaults.
- **Windows only.** The plugin binary targets Windows and reads Windows-specific paths (RSI Launcher logs, `%APPDATA%`).

---

## FAQ

**Is this against Star Citizen's TOS?**
The plugin simulates keyboard and mouse input — the same kind of input the OS would deliver from your physical keyboard. It does not hook the game process, modify `Data.p4k`, or read game memory. *Generate Binds* writes a keybinding profile you import yourself through the in-game options. That said, no third-party tool is officially supported — use at your own risk.

**Does it break after every patch?**
The action list is regenerated from your current `Data.p4k`, so it re-targets whatever the new build ships. If action IDs change between patches, you may need to re-pick a few buttons.

**It says no installations were found.**
The plugin discovers installs by parsing the `Launching {Version}` line out of the RSI Launcher's log. Each channel (LIVE, PTU, EPTU, TECH-PREVIEW) only shows up after you've actually launched the game from that channel at least once — opening the launcher isn't enough.

**Why doesn't my mouse-button binding fire while I'm flying / aiming?**
EasyAntiCheat blocks synthetic mouse-button input while the game is in relative-mouse mode. Bind those actions to a keyboard key instead.

---

## A note on AI assistance

Parts of this codebase — and this README — were written with the help of AI tools (primarily Claude Code). Every change is reviewed before it lands. I want to be upfront about it rather than pretend otherwise. If something reads or behaves oddly, an issue or PR is very welcome.

---

## For developers

The rest of this README covers building from source. End users don't need any of it — grab the `.streamDeckPlugin` from the releases page above.

### Stack

- **Language:** Rust (stable, 2024 edition)
- **Framework:** [`streamdeck-lib`](https://github.com/veelume/streamdeck-lib)
- **Game-data extraction:** [svarog](https://github.com/19h/svarog) (P4K + CryXML)
- **Rendering:** [`streamdeck-render`](https://github.com/veelume/streamdeck-render) (PNG button faces)
- **Property Inspector:** [sdpi-components](https://github.com/GeekyEggo/sdpi-components) (local, no CDN)

### Prerequisites

- [Rust](https://rustup.rs/) stable
- [Elgato CLI](https://docs.elgato.com/streamdeck/sdk/introduction/getting-started) (`npm install -g @elgato/cli`)

### Build & run

```sh
git clone https://github.com/VeeLume/streamdeck-starcitizen.git
cd streamdeck-starcitizen
cargo build --release

# One-time: link the sdPlugin folder into Stream Deck for development
streamdeck link icu.veelume.starcitizen.sdPlugin

# After each rebuild — stop first so Windows isn't holding the exe locked
streamdeck stop icu.veelume.starcitizen
cp target/release/plugin.exe icu.veelume.starcitizen.sdPlugin/bin/icu.veelume.starcitizen.exe
streamdeck restart icu.veelume.starcitizen
```

VS Code users: the **Build Plugin** task (`Ctrl+Shift+B`) does the stop → build → copy → restart cycle for you.

### Release

This project uses [Conventional Commits](https://www.conventionalcommits.org/) (enforced by a `commit-msg` hook) and [git-cliff](https://git-cliff.org/) for automated changelogs.

```sh
cargo install cargo-release git-cliff   # one-time
cargo release patch                     # or: minor, major
```

GitHub Actions then builds, packs, validates, and publishes a GitHub Release with notes grouped by commit type.

### Architecture

```
RSI Launcher log  →  discovery  →  Installations store
                                        ↓
Data.p4k  →  svarog-p4k  →  defaultProfile.xml  ┐
                            global.ini          ├→  bindings parser  →  Bindings store
actionmaps.xml (user)  ────────────────────────┘                            ↓
                                                                       Actions read & render
                                                                            ↓
                                                                       PNG button face
                                                                            ↓
                                                                       Stream Deck
```

Four pillars: **Actions** (per-key logic), **Adapters** (background workers like the file watcher), **Extensions** (shared state stores), **Topics** (typed pub/sub bus). For the detailed breakdown — action lifecycle, topic targeting, how to add a new action — see [CLAUDE.md](CLAUDE.md).

For Star Citizen-specific domain knowledge (how bindings, installations, and translations actually work in SC), see [docs/star-citizen-domain.md](docs/star-citizen-domain.md).

---

## Acknowledgements

- [streamdeck-starcitizen](https://github.com/mhwlng/streamdeck-starcitizen) by mhwlng — the original C# plugin and the reason this one exists.
- [SCStreamDeck](https://github.com/Jarex985/SCStreamDeck/) by Jarex985 — another Stream Deck plugin for SC.
- **[KVT KORP](https://ko-fi.com/kvtkorp/shop)** — the [Kommand Kontrol Kit (SCK3)](https://ko-fi.com/s/f08ba27204), a free Star Citizen icon set and Stream Deck profile, sparked the idea for the icon matcher and the PNG button renderer in this plugin. Worth the recommended donation if you use it.
- **UAV OSD Sans Mono** by [Nicholas Kruse](https://nicholaskruse.com) ([font page](https://nicholaskruse.com/work/uavosd)) — the bundled font used for rendered buttons. Free for personal and commercial use, per the author.
- [svarog](https://github.com/19h/svarog) by [@19h](https://github.com/19h) — Rust library for reading SC's `Data.p4k` and converting CryXML.
- [sdpi-components](https://github.com/GeekyEggo/sdpi-components) by [@GeekyEggo](https://github.com/GeekyEggo) — web-component library for Property Inspector UIs.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
