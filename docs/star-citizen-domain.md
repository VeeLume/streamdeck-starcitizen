# Star Citizen Domain Knowledge

> This document describes the Star Citizen systems the plugin interfaces with.
> Understanding this domain is essential for developing and maintaining the plugin.

---

## 1. Installations and Channels

Star Citizen can have multiple installations on the same machine, each corresponding to a release **channel**: LIVE (the stable release), PTU (public test), EPTU (experimental test), Hotfix, and TechPreview. A player may have anywhere from one to all five installed simultaneously.

The RSI Launcher logs every game launch to `%APPDATA%/rsilauncher/logs/log.log`. This log is the only reliable way to discover which installations exist and where they are. There is no registry key, no config file, no API — just the log.

The log has two formats:

- **Legacy:** `[Launcher::launch] Launching Star Citizen LIVE from (D:\StarCitizen\LIVE)`
- **v2.x (JSON):** `{ "t":"...", "[main][info] ": "Launching Star Citizen LIVE from (C:\\Games\\StarCitizen\\LIVE)" }`

The parser extracts all launch entries from the log, deduplicates by channel (keeping the last-seen path for each), and skips entries where the directory no longer exists on disk.

Each installation directory also contains a `build_manifest.id` JSON file with metadata. This file also has two formats:

- **New (v2):** `{ "Data": { "Version": "4.6.173.39432", "Branch": "sc-alpha-4.6.0", "BuildId": "..." } }`
- **Legacy:** `{ "RequestedP4kFileName": "Data_4.6.1.0.p4k", "Branch": "...", "BuildId": "..." }` (version is extracted from the P4K filename)

Channels have a natural priority for default selection: LIVE > Hotfix > PTU > EPTU > TechPreview. When the plugin starts, it selects the highest-priority available installation automatically.

## 2. The Keybinding System

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

The plugin respects these timing parameters when simulating key presses; otherwise the game won't recognize the action.

## 3. UI Translations

Action and category labels in the binding files are not human-readable by default. They use `@`-prefixed localization keys like `@ui_CIToggleMiningMode` or `@ui_CGSpaceFlight`. The actual translated strings are stored in `Data/Localization/english/global.ini`, which is also embedded inside `Data.p4k`.

When a translation lookup fails (which happens for some actions), the raw key is humanized algorithmically: strip the `@` and `ui_` prefix, split on underscores and camelCase boundaries, and title-case each word. For example: `@ui_CGSeatGeneral` → "CG Seat General", `v_power_toggle` → "V Power Toggle".

Some labels also carry " (Short Press)" or " (Long Press)" suffixes that are stripped for display.

**For button rendering**, many SC action words are too long to fit on a 144x144 pixel button face. An abbreviation table (~60 entries) shortens common words: ACCELERATION→ACCEL, QUANTUM→QTM, WEAPONS→WPNS, TOGGLE→TOGL, SPACESHIP→SHIP, TARGETING→TGT, etc.

## 4. The "No Keyboard Bind" Problem

This is the core problem the plugin solves. Hundreds of SC actions have no keyboard shortcut assigned by default. Since a Stream Deck button can only simulate keyboard presses, these actions are completely inaccessible without first assigning them a keyboard shortcut.

Doing this manually for hundreds of actions is impractical, so the plugin has an **autofill system** that generates conflict-free keyboard bindings for all unbound actions. The generated bindings are written as an `<ActionMaps>` XML file that the game can import through its keybind settings UI.

The generation algorithm handles several constraints:

- **Category groups:** Some categories can be active simultaneously (e.g. SpaceFlight + SeatGeneral + MFDs are all active while flying). Actions within the same group must not share key combinations.
- **Modifier detection:** Some modifier keys (like LShift or RAlt) are used as *main keys* by certain bindings (e.g. RShift bound to "Lock Pitch"). These modifiers are excluded from generated combinations in related categories to avoid conflicts.
- **Axis filtering:** Actions bound to analog inputs (thumbsticks, joystick axes) are skipped entirely — they can't be meaningfully triggered by a digital key press.
- **Full modifier space:** With 6 modifier keys (LShift, RShift, LCtrl, RCtrl, LAlt, RAlt), the system generates all 2^6 = 64 modifier combinations per candidate key to maximize the available key space.

## 5. Memory Considerations

The binding data is large. With 1,000+ actions, each having multiple bindings, labels, and activation mode references, naive string storage leads to significant duplication. Action names and UI labels appear thousands of times across the data model. **String interning** (sharing a single allocation across all references to the same string) is essential to keep memory usage reasonable.

---

## Open Design Decisions

These are features that could be evaluated for future development:

**Execution via adapter pattern.** Rather than actions calling the key simulation directly, actions could publish execution requests to a pub/sub topic and a background adapter could handle the actual key press. This would cleanly enable future interceptors (audio cues, logging, macro recording) without touching action code.

**Per-action hold duration override.** Currently timing comes entirely from the binding's activation mode. Some users may want to override the hold duration per button (e.g. type in 500ms). This could be an optional field that, when set, overrides the activation mode timing.

**Raw key-down/key-up mode.** Instead of a full press/release cycle, some use cases (continuous actions, flight controls) may benefit from separate key-down and key-up events tied to the Stream Deck button state.

**Semantic index.** An ML-powered embedding index for smarter label and icon matching. Would require a ~33 MB model download and peaks at ~850 MiB RAM during build. The token-based matcher is sufficient for daily use, so this should remain optional if implemented.

**Bind generator PI options.** The autofill generator has several configurable parameters (candidate keys, candidate modifiers, denied combos, skipped action maps, category group definitions, whether to include user binds as occupied slots) that are currently hardcoded or use defaults. Exposing these in the Generate Binds Property Inspector would let advanced users fine-tune the generation without editing config files.

---

## Glossary

| Term | Meaning |
|------|---------|
| P4K | SC's proprietary archive format (`Data.p4k`, the game's main data file) |
| CryXML | CryEngine's binary XML format, must be converted to text before parsing |
| Action Map | A category of game actions (e.g. spaceship, on-foot, vehicle) |
| Action Binding | A single game command with its key/button assignments and activation mode |
| Activation Mode | How a key triggers an action: instant press, hold (with timing), release, double-tap |
| PI | Property Inspector — the HTML settings panel for configuring a Stream Deck key |
| SDPI | Elgato's JS component library for building property inspectors |
| Channel | A SC release branch: LIVE, PTU, EPTU, HOTFIX, TechPreview |
| Category Group | A set of action maps that can be active simultaneously and must not share key combos |
