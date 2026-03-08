# Star Citizen Tools — Stream Deck Plugin Spec

> **Project:** A Stream Deck plugin that bridges Star Citizen's keybinding system with Elgato Stream Deck hardware, allowing players to execute any in-game action from a physical button.
>
> **Template:** `D:\Users\Valerie\AppData\Local\Microsoft\PowerToys\NewPlus\Vorlagen\StreamDeck Plugin (Rust)`

| Field | Value |
|---|---|
| Plugin ID | `icu.veelume.starcitizen` |
| Plugin Name | Star Citizen Tools |
| Author | Veelume |
| Platform | Windows only |
| SD SDK Version | 3 |
| Monitored App | `StarCitizen.exe` |

---

## Part 1 — How Star Citizen Works

This section describes the Star Citizen systems the plugin needs to interface with. Understanding this domain is essential because it dictates why the plugin exists and what problems it solves.

### 1.1 Installations and Channels

Star Citizen can have multiple installations on the same machine, each corresponding to a release **channel**: LIVE (the stable release), PTU (public test), EPTU (experimental test), Hotfix, and TechPreview. A player may have anywhere from one to all five installed simultaneously.

The RSI Launcher logs every game launch to `%APPDATA%/rsilauncher/logs/log.log`. Each launch produces a line like:

```
[Launcher::launch] Launching Star Citizen LIVE from (D:\StarCitizen\LIVE)
```

This log is the only reliable way to discover which installations exist and where they are. There is no registry key, no config file, no API — just the log.

Each installation directory also contains a `build_manifest.id` JSON file with metadata: the version string (e.g. `4.6.1.0`), branch name, build date, and build ID. This is useful for display and for detecting when the game has been updated.

Channels have a natural priority for default selection: LIVE > Hotfix > PTU > EPTU > TechPreview. When the plugin starts, it should select the highest-priority available installation automatically.

### 1.2 The Keybinding System

Star Citizen has an unusually complex keybinding system. There are over 1,000 bindable actions spread across dozens of categories ("action maps"), and bindings are stored in a two-layer overlay:

**Layer 1 — Default profile.** The file `defaultProfile.xml` contains every action the game supports with its factory-default bindings. This file is embedded inside `Data.p4k`, a proprietary archive format unique to Star Citizen. To make matters worse, the XML inside the archive is not plain text — it's in CryEngine's binary XML format (CryXmlB), which must be converted to text XML before it can be parsed.

**Layer 2 — User customizations.** The file `{INSTALL}/user/client/0/Profiles/default/actionmaps.xml` contains only the bindings the player has changed. It's a sparse overlay — most actions aren't mentioned at all. These customizations are merged on top of the defaults.

**Action maps** are categories like "Spaceship - General", "On Foot - All", "Vehicles - Ground". Each action map contains a set of **actions** (individual commands like "Toggle Power", "Fire Weapon Group 1", "Open MobiGlas"). Multiple action maps can share the same translated display name (e.g. "On Foot - All" groups the player, prone, tractor_beam, and incapacitated maps), meaning their actions should be presented together to the user.

Each action can have bindings for multiple input devices: keyboard, mouse, gamepad, and joystick. A single action may have a keyboard shortcut, a joystick button, and a gamepad trigger all assigned simultaneously.

**Activation modes** define *how* a key triggers an action. The modes are pooled centrally (not duplicated per action) and include:

- **Press** — fires immediately when the key goes down
- **Hold** — fires after the key has been held for a configurable delay (`holdTriggerDelay`), optionally repeating at a second interval (`holdRepeatDelay`)
- **Release** — fires when the key is released
- **Double-tap** — fires on a rapid second press

The plugin must respect these timing parameters when simulating key presses; otherwise the game won't recognize the action.

### 1.3 UI Translations

Action and category labels in the binding files are not human-readable by default. They use `@`-prefixed localization keys like `@ui_CIToggleMiningMode` or `@ui_CGSpaceFlight`. The actual translated strings are stored in `Data/Localization/english/global.ini`, which is also embedded inside `Data.p4k`.

When a translation lookup fails (which happens for some actions), the raw key needs to be humanized algorithmically: strip the `@` and `ui_` prefix, split on underscores and camelCase boundaries, and title-case each word. For example: `@ui_CGSeatGeneral` → "CG Seat General", `v_power_toggle` → "V Power Toggle".

Some labels also carry " (Short Press)" or " (Long Press)" suffixes that should be stripped for display.

**For button rendering**, many SC action words are too long to fit on a 144×144 pixel button face. An abbreviation table (~60 entries) is needed to shorten common words: ACCELERATION→ACCEL, QUANTUM→QTM, WEAPONS→WPNS, TOGGLE→TOGL, SPACESHIP→SHIP, TARGETING→TGT, etc.

### 1.4 The "No Keyboard Bind" Problem

This is the core problem the plugin solves. Hundreds of SC actions have no keyboard shortcut assigned by default — they're only bound to joystick or gamepad inputs. Since a Stream Deck button can only simulate keyboard presses (not joystick inputs), these actions are completely inaccessible without first assigning them a keyboard shortcut.

Doing this manually for hundreds of actions is impractical, so the plugin needs an **autofill system** that generates conflict-free keyboard bindings for all unbound actions. The generated bindings must be written as an `<ActionMaps>` XML file that the game can import through its keybind settings UI.

The generation algorithm must handle several constraints:

- **Category groups:** Some categories can be active simultaneously (e.g. SpaceFlight + SeatGeneral + MFDs are all active while flying). Actions within the same group must not share key combinations.
- **Modifier detection:** Some modifier keys (like LShift or RAlt) are used as *main keys* by certain bindings (e.g. RShift bound to "Lock Pitch"). These modifiers must be excluded from generated combinations in related categories to avoid conflicts.
- **Axis filtering:** Actions bound to analog inputs (thumbsticks, joystick axes) should be skipped entirely — they can't be meaningfully triggered by a digital key press.
- **Full modifier space:** With 6 modifier keys (LShift, RShift, LCtrl, RCtrl, LAlt, RAlt), the system should generate all 2^6 = 64 modifier combinations per candidate key to maximize the available key space.
- **Configurable:** While defaults should be sensible, the user should be able to override candidate keys, candidate modifiers, denied combos (e.g. Alt+F4), skipped action maps, and category group definitions.

### 1.5 Memory Considerations

The binding data is large. With 1,000+ actions, each having multiple bindings, labels, and activation mode references, naive string storage leads to significant duplication. Action names and UI labels appear thousands of times across the data model. **String interning** (sharing a single allocation across all references to the same string) is essential to keep memory usage reasonable.

---

## Part 2 — How Stream Deck Plugins Work

The plugin template (`StreamDeck Plugin (Rust)`) already contains comprehensive documentation in its `CLAUDE.md`. This section provides only the high-level concepts needed to understand the plugin design. **Refer to the template's CLAUDE.md for the full framework reference.**

### 2.1 Core Concepts

A Stream Deck plugin is a long-running background process that communicates with the Stream Deck application over a local WebSocket. The plugin framework (`streamdeck-lib`) provides four building blocks:

**Actions** — Per-key logic. Every Stream Deck button that uses one of the plugin's action types gets its own action instance. Actions handle key presses, render button faces, and communicate with their settings panel. They live in `src/actions/`.

**State (Extensions)** — Shared data stores registered at startup, accessible from any action via the context object. Thread-safe. They live in `src/state/`.

**Topics (Pub/Sub)** — Typed event channels that decouple components. One action can publish a "bindings changed" event without knowing which other actions care about it. Defined in `src/topics.rs`.

**Adapters** — Optional background workers for I/O or long-running tasks. They live in `src/adapters/` and can subscribe to topics.

### 2.2 Button Rendering

Stream Deck buttons are 144×144 pixels. The SDK provides two overlapping rendering mechanisms: `set_image` (a full-bleed image) and `set_title` (a text overlay). These render in separate layers with no layout control, so **mixing them causes clipping and visual inconsistency**.

The correct approach is to render the entire button face as a single PNG image using the `streamdeck-render` crate. This gives full control over text placement, font rendering, and compositing. The font used is **UAV OSD Sans Mono** by Nicholas Kruse — monospace, clean at small sizes, bundled in the plugin's `fonts/` directory.

Each action has **three icon states** defined in the manifest:

1. **Loading** — a dimmed SVG shown before the plugin process connects
2. **Active** — a full-brightness SVG, used only as a brief fallback if the font fails to load
3. **Data** — a programmatic PNG rendered on every state change (the normal operating state)

### 2.3 Property Inspector (PI)

Each action type has an HTML settings panel (the Property Inspector) that appears when the user configures a button. PIs use Elgato's `sdpi-components.js` library (v4, bundled locally).

Communication between the PI and the Rust action is bidirectional: SDPI components auto-persist settings; the plugin can push status updates and dynamically populate dropdowns; the PI can send custom messages for actions like "browse folder" or "trigger build".

### 2.4 Application Monitoring

The Stream Deck SDK can monitor application launches. The plugin should monitor `StarCitizen.exe` to detect when the game starts (triggering an installation refresh and channel detection) and stops.

---

## Part 3 — Bridging the Gap

This section describes what the plugin needs to do to connect Star Citizen's systems with Stream Deck hardware. It defines 4 actions, the shared state they rely on, and the event system that ties them together.

### 3.1 Architecture

The plugin should be structured in layers:

1. **Installation discovery** — parse the RSI Launcher log, detect channels, read build manifests
2. **Binding parsing** — extract and parse the default profile from Data.p4k, load translations, apply user overlay
3. **Binding execution** — convert a parsed binding into an OS-level key simulation
4. **Bind autofill** — generate missing keyboard shortcuts
5. **Semantic index** (optional, deferred) — ML-powered label and icon matching
6. **Plugin** — actions, state, property inspectors, event bus

Start as a single crate and extract library crates only when the code is large enough to justify it.

#### Key Dependencies

| Dependency | Purpose |
|---|---|
| `streamdeck-lib` (veelume, v0.5.0) | SD protocol, action framework, input execution, pub/sub bus |
| `streamdeck-render` (veelume, v0.1.3) | PNG icon rasterization with custom fonts |
| `svarog-p4k` + `svarog-cryxml` (19h/Svarog, v1.4.0) | Read .p4k archives and convert CryXML to text |
| `roxmltree` | XML parser for binding files |
| `fastembed` (v5) | Sentence-embedding model (optional, for semantic index) |

### 3.2 Shared State

All state is registered at plugin startup and accessible from any action.

**Active Installation State** — Tracks all discovered installations, the current selection index, and the last launched channel. Supports next/previous rotation, selection by channel, and refresh (re-scan from launcher log). Thread-safe.

**Bindings State** — Holds the loaded binding data, the channel it was loaded from, and any error from the last load attempt. Provides scoped read access so callers don't hold the lock longer than needed. Loading involves extracting from P4K, converting CryXML, parsing XML, loading translations, and applying user overlay.

**Font State** — Loaded once at startup from the plugin's `fonts/` directory. Immutable after load. Used by all button rendering.

**Icon Folder State** — A user-configured path to a folder of icon files (PNG, SVG, GIF). Written by the Settings action, read by Execute Action.

**Semantic Index State** (optional) — Holds a precomputed embedding index for smarter label and icon matching. Can be loaded from disk, replaced after a build, or absent entirely. Default path: `%APPDATA%\icu.veelume.starcitizen\semantic-index.json`.

### 3.3 Event System

Actions should be decoupled through typed pub/sub topics:

- **Installation Changed** — published when the active installation switches. Carries channel, label, version, index, total count.
- **Bindings Reloaded** — published after bindings are loaded (or fail). Carries channel, map count, action count, success flag, error.
- **Icon Folder Changed** — published when the global icon folder setting changes.
- **Installations Refreshed** — published after a full re-scan. Carries count and channel list.
- **Index Changed** — published after a semantic index build completes.

### 3.4 Application Hooks

**On StarCitizen.exe launch:** Detect which channel was launched (by matching PTU/EPTU/HOTFIX/TECH in the process name), record it as "last launched", and refresh the installation list.

**On StarCitizen.exe terminate:** Log only.

**On global settings change:** Sync the icon folder state; notify all Execute Action buttons if the folder changed.

---

## 4. Action Specifications

### 4.1 Manage Version

**Purpose:** Display and manage which Star Citizen installation is active.

A single action with three behavioral modes, configured per-button instance:

**Show mode** — read-only display of the currently active installation. Renders the channel name (e.g. "LIVE") and a shortened version number (e.g. "4.6"). Pressing the button refreshes the display.

**Pin mode** — locks the button to a specific channel chosen via a dropdown in the PI. Renders "set" above the channel name, with the name at full brightness when that channel is currently active or dimmed when it isn't. Pressing switches to that channel and reloads bindings.

**Cycle mode** — rotates through all available installations. Renders the current channel + version, a separator line, and a dimmed "→ NEXT" preview. Pressing advances to the next installation. On Stream Deck+ hardware, the encoder dial rotates forward/backward through installations.

**PI:** Receives status updates (mode, channel, version, next channel, total count). Provides a dynamic dropdown for the channel list (Pin mode).

**Events:** Subscribes to Installation Changed to update display when the active installation changes elsewhere.

### 4.2 Execute Action

**Purpose:** Execute a Star Citizen keybind from a Stream Deck button, with optional hold and double-press actions.

This is the primary action and the most complex.

**Action selection:** The user first picks a category (action map) from a dropdown, then picks a specific action within that category. When multiple action maps share the same translated label, their actions should be merged into a single list. Actions ending in `_long` should be hidden (they're duplicates of `_short` variants handled internally by activation modes).

**Press behavior — four execution paths:**

1. **Fast path** (no hold or double-press configured): Fire the primary action immediately on key-down. Zero latency — this is the common case and must feel instant.
2. **Hold only:** Measure hold duration. If held ≥ threshold, fire the hold action. If released quickly, fire the primary.
3. **Double-press only:** On key-up, start a timer. If the user presses again within the window, fire the double-press action. If the timer expires, fire the primary (with a slight delay equal to the window).
4. **Both configured:** Hold takes priority. If held long enough, fire hold action regardless. For quick presses, double-press logic applies.

**Keybinding execution:** Find the action's keyboard binding, convert it to a key + modifiers combination, and simulate the key press via Windows scan codes. Respect the action's activation mode timing — for hold actions, hold the simulated key for the configured duration plus a small buffer before releasing.

**Button display — priority cascade:**

1. Custom icon from the icon folder (if a filename is selected and the file loads successfully)
2. Custom title text (if manually entered in the PI)
3. Semantic index short label (if the index is loaded)
4. Auto-derived label from the binding's translated UI label
5. Fallback static image

For label rendering: uppercase, abbreviate common SC words (~60 rules), word-wrap to fit up to 3 lines on the button. Two border styles: solid rounded-rect stroke or a vignette glow effect.

**Icon matching (for auto-suggesting icons from the user's icon folder):**

1. *Token fuzzy matching* — Tokenize the action ID and its label, score each icon filename by exact and prefix overlap. Label tokens should be weighted higher than action-ID tokens (icon designers name files after the function, not the internal path). Prefix matching bridges abbreviation gaps (e.g. `mfd` ↔ `mfds`).
2. *Semantic re-ranking* (optional, if index loaded) — Blend token scores with cosine similarity from precomputed embeddings to handle vocabulary gaps (e.g. `IFCS` ↔ `flight-control-system`).

**PI:** Three cascading category→action pickers (primary, hold, double-press). Hold/double-press sections with enable/disable toggles and timing threshold inputs. Icon picker dropdown. Real-time display of the resolved label and bind description. Dynamic datasources for all dropdowns, with hot-reload when categories change.

**Events:** Subscribes to Bindings Reloaded, Icon Folder Changed, and Index Changed to refresh display.

### 4.3 Settings

**Purpose:** Plugin-wide configuration, installation refresh, and semantic index management.

Manages three global settings that affect all other actions:

- **Auto-load bindings** (default on): Automatically reload bindings when the active installation changes
- **Auto-select last launched** (default off): Switch to whichever channel was most recently launched
- **Icon folder** (optional): Path to a folder of icon files for Execute Action buttons

Settings need a per-action ↔ global sync mechanism: the SDPI components write to the button's per-action settings, which are promoted to global settings so every other action can read them. On appearance, the action should hydrate from globals so it shows current values even if a different Settings button changed them.

**Press behavior:** Refreshes the installation list and reloads bindings (if auto-load is enabled).

**Semantic index build** (optional, deferred feature): Triggered from the PI. Runs in a background thread. Progress should be rendered directly onto the button face as changing PNG labels (e.g. STARTING → MODEL DL % → EMBED ACTIONS n/n → DONE). Must be abortable and panic-safe. On completion, the index is swapped into shared state, saved to disk, and all Execute Action buttons are notified.

**PI:** Shows installation count, selected channel, bindings summary. Buttons for refresh, folder browse, index build/abort.

### 4.4 Generate Binds

**Purpose:** Run the bind autofill generator and save the resulting XML to disk.

**Press behavior:** Resolve the output directory (explicit path from settings, or auto-derived as `{install}/user/client/0/controls/mappings/`). Clone the current bindings, build the generator config, run the generator, render XML, write to disk. The output filename should be stable (e.g. `icu-veelume-starcitizen.xml`) so repeated generations overwrite rather than accumulate.

**Settings:** Output path override, profile name, auto-detect deny modifiers toggle, deny combos list, skip action maps list, candidate keys/modifiers (empty = use defaults), and whether to include user binds as occupied slots.

**PI:** Shows bindings summary, resolved output path, and last generation result (count or error). Button for browsing a custom output path.

**Events:** Subscribes to Installation Changed and Bindings Reloaded to update display.

---

## 5. Rendering Rules

**Never mix `set_image` with `set_title` on the same button.** Stream Deck renders these in separate layers with no layout control. Every button must render its entire face as a single self-contained PNG.

**Icon lifecycle per action:**
1. **Loading** — dimmed SVG from the manifest (before plugin connects)
2. **Active** — full-brightness SVG (brief fallback if font fails)
3. **Data** — programmatic PNG on every state change (normal operation)

**Render modes needed:**

- Channel name + optional short version (Version Show)
- Current channel + separator + "→ NEXT" preview (Version Cycle)
- "set" label + channel name at variable brightness (Version Pin)
- Word-wrapped, abbreviated, bordered label (Execute Action, Settings, Generate Binds)

**Two border styles for labels:**
- Solid — rounded-rect stroke
- Glow — vignette that fades inward from the border

---

## 6. Open Design Decisions

These are features that should be evaluated during development:

**Execution via adapter pattern.** Rather than actions calling the key simulation directly, actions could publish execution requests to a pub/sub topic and a background adapter could handle the actual key press. This would cleanly enable future interceptors (audio cues, logging, macro recording) without touching action code.

**Per-action hold duration override.** Currently timing comes entirely from the binding's activation mode. Some users may want to override the hold duration per button (e.g. type in 500ms). This could be an optional field that, when set, overrides the activation mode timing.

**Raw key-down/key-up mode.** Instead of a full press/release cycle, some use cases (continuous actions, flight controls) may benefit from separate key-down and key-up events tied to the Stream Deck button state.

**Semantic index priority.** The semantic index adds smart labels and vocabulary-gap icon matching, but it requires a ~33 MB ML model download and peaks at ~850 MiB RAM during build. It should be a cargo feature flag, not a default dependency. The token-based matcher is sufficient for daily use.

---

## 7. Implementation Order

1. Scaffold from template — plugin ID, manifest with all 4 action stubs
2. Installation discovery — parse launcher log, detect channels, read manifests
3. Binding parsing — P4K extraction, CryXML conversion, XML parsing, overlay, translations
4. Shared state — installations, bindings, font
5. Version action — all 3 modes + PNG rendering (validates the full render pipeline)
6. Binding execution — key mapping, modifier conversion, activation mode timing, OS-level simulation
7. Execute action — start with fast-path only, then add hold + double-press
8. Settings action — global sync, refresh, icon folder picker
9. Bind autofill — the generator and its configuration
10. Generate Binds action — wire up the generator
11. Icon matching — token fuzzy matcher
12. PI HTML — one file per action
13. Application hooks — StarCitizen.exe launch/terminate monitoring
14. Event wiring — pub/sub for cross-action state sync
15. (Optional) Semantic index — behind a feature flag

---

## 8. Plugin Bundle Layout

```
icu.veelume.starcitizen.sdPlugin/
├── bin/                               ← compiled binary
├── fonts/                             ← UAV OSD Sans Mono for label rendering
├── images/
│   ├── starting/                      ← loading-state icons (dimmed)
│   ├── active/                        ← fallback icons (font-load failure only)
│   ├── plugin.png / plugin@2x.png     ← store listing icon (256/512)
│   ├── category.svg                   ← sidebar category icon (28×28, mono white)
│   └── {action}.svg                   ← per-action icons (20×20, mono white)
├── pi/
│   ├── sdpi-components.js             ← SDPI components v4 (bundled)
│   └── {action}.html                  ← one PI per action
└── manifest.json
```

**Manifest requirements:** Each action entry needs a UUID matching the Rust action ID, a PI path, controller types (Keypad; additionally Encoder for Version), and a loading-state image. Version and Execute should support multi-actions; Settings and Generate Binds should not.

---

## 9. Performance Targets

- Idle RSS: ~5 MiB (without semantic model)
- Binding parse: < 500ms (P4K extraction + CryXML conversion + XML parsing)
- Button render: < 5ms per PNG
- Key simulation latency: imperceptible (< 10ms from key-down to OS event)
- Semantic index build peak: ~850 MiB (acceptable since it's user-triggered and transient)
- Include lightweight memory logging (RSS snapshots at key lifecycle points) for diagnostics

---

## 10. Glossary

| Term | Meaning |
|---|---|
| P4K | SC's proprietary archive format (`Data.p4k`, the game's main data file) |
| CryXML | CryEngine's binary XML format, must be converted to text before parsing |
| Action Map | A category of game actions (e.g. spaceship, on-foot, vehicle) |
| Action Binding | A single game command with its key/button assignments and activation mode |
| Activation Mode | How a key triggers an action: instant press, hold (with timing), release, double-tap |
| PI | Property Inspector — the HTML settings panel for configuring a Stream Deck key |
| SDPI | Elgato's JS component library for building property inspectors |
| Channel | A SC release branch: LIVE, PTU, EPTU, HOTFIX, TechPreview |
| Category Group | A set of action maps that can be active simultaneously and must not share key combos |
