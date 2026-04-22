# Roadmap

Living document. Tracks tiered feature work and splits scope between the plugin
itself and the planned Tauri companion app. Feature entries linked to
`docs/features/*.md` have a written spec; unlinked entries are placeholders
awaiting a spec.

## Guiding split: plugin vs. companion app

The Stream Deck plugin is runtime: it reacts to button presses, renders buttons,
executes bindings, and watches files. The companion app (Tauri, see
[companion-app.md](companion-app.md)) is authoring time: it produces and edits
configuration consumed by the plugin.

| Goes in the plugin                                             | Goes in the companion app                        |
| -------------------------------------------------------------- | ------------------------------------------------ |
| Action execution (keyboard, mouse, dial)                       | Style editor with live button preview            |
| Button rendering                                               | Label mode editor                                |
| Installation discovery + launcher log parsing                  | Toggle-group editor                              |
| Binding parse / overlay / watch / autofill generation          | Macro / chain builder (timeline UI)              |
| Telemetry adapter(s) (game.log tailer)                         | Generator tuning (candidate keys, deny combos)   |
| Property Inspector UIs (per-action settings only)              | Profile import/export workflow                   |
| Safety / diagnostics (admin check, failure alerts)             | Global/quick settings UI                         |

Design principle: the plugin stays self-sufficient without the companion app.
Default configs ship with the plugin; the companion app is for users who want
to customize beyond the defaults or who edit a lot at once.

PI scope:
- Per-key settings always live in the action's PI.
- The Settings action stays as the global entry point. It keeps a small set of
  global settings users change quickly or often, and provides a button to
  launch the companion app for the rest.
- More complex or infrequently-touched configuration (style authoring, label
  pipelines, toggle groups, generator tuning, macros) lives in the companion.

## Tiering

- **T1** — high leverage, clear path, committed scope.
- **T2** — solid value, moderate effort, design still open.
- **T3** — incremental polish or blocked on larger work.
- **T0** — bug fixes that should land before new work.

## T0 — Pre-work fixes

### Channel detection broken for `on_game_launch`
*Location:* [src/discovery.rs:220](../src/discovery.rs#L220), called from
[src/main.rs:121](../src/main.rs#L121).

`detect_channel_from_app` parses the executable name, but every channel ships
as `StarCitizen.exe` — so the detector always returns `Live`. This makes the
"auto-select last-launched" setting effectively a "always select LIVE" setting.

Fix: delete `detect_channel_from_app`. In `on_game_launch`, read the latest
entry from the RSI launcher log (`parse_launcher_log` already returns entries
in order) and use that channel. Handle the zero-entry case (log was cleared or
game was started outside the launcher) by leaving `last_launched` unchanged.

Spec: [features/channel-detection-fix.md](features/channel-detection-fix.md)
(to write).

## T1 — Plugin features

### Mouse-button execution
Turned out to be mostly done already: parser, overlay, executor, and the
underlying `streamdeck-lib` input layer all handle mouse buttons and scroll.
The only real bug is `resolve_combo` in `src/actions/shared.rs` filtering
`Device::Keyboard` only, so mouse-bound actions silently no-op at runtime.

Spec: [features/mouse-execution.md](features/mouse-execution.md).

## T2 — Plugin features

### Stream Deck+ dial action
Biggest competitive gap, but the UX question is larger than first scoped:
how to choose which interactions (rotate / press / tap / long-tap) map to
which bindings, how to surface common CW/CCW pairs, how rendering works on
the Encoder layout, and how/whether `ToggleAction` should grow dial support
too. Needs another design pass before implementation.

Approach (tentative): extend `ExecuteAction` to declare both `Keypad` and
`Encoder` controllers rather than adding a new action type. First draft of
the design lives in [features/dial-action.md](features/dial-action.md); treat
it as a starting point, not a committed plan.

### Game-log telemetry adapter (coarse events only)
Tail the active installation's `Game.log` and emit typed topics for events
that actually appear:

- Quantum travel: target selected, route calculated (with fuel estimate),
  arrival.
- Login / session / character.
- Zone transitions (Armistice enter/leave).

**Not viable**: per-item-port state (landing gear, power, doors, shields,
cargo). Star Citizen does not log these. Toggle-action sync via logs is off
the table.

Shape: a new `GameLogAdapter` (OnAppLaunch policy) reading the latest
`<install>/Game.log`, publishing topics such as `QT_STATE_CHANGED`,
`ZONE_CHANGED`, `SESSION_CHANGED`. New actions (e.g. "QT status") can
subscribe; existing actions ignore it.

### Audio feedback per action
Optional WAV playback on `key_down`. Small crate (`rodio`). Per-action PI
control for file path; silent by default.

## T2 — Companion app (tracked separately)

Tauri desktop companion app. See [companion-app.md](companion-app.md) (to
write) for details. Initial scope candidates:

- Live button preview using `streamdeck-render` directly.
- Style editor (colors, fonts, borders).
- Label mode editor.
- Toggle-group editor.
- Macro / chain builder.
- Generator tuning (candidate keys, deny combos, category groups).
- Profile import into SC (`Controls/Mappings/`).
- Global / quick settings that don't belong per-key.

The companion app is large and gets its own design pass before implementation
starts.

## T3 — Plugin features

### Searchable action dropdown
Not built into sdpi-components. Needs a custom web component wrapping a text
filter over a datasource-backed select. Real value with 1000+ actions but
nontrivial build; defer until other T1/T2 work lands.

### `global.ini` override folder
Allow users to drop a custom `global.ini` (e.g. community German translation)
into `%APPDATA%/icu.veelume.starcitizen/localization/<lang>/` to override the
one from `Data.p4k`. Cheap extension to the existing translations module.

### Misc
- Expose advanced autofill settings in Generate Binds PI (only after we decide
  whether this moves to the companion app instead).
- One-click profile import into SC after Generate Binds completes.

## Deferred

### Admin-elevation detection + warning
Deferred because clean detection needs an `InputAdapter` failure topic or
similar surface in `streamdeck-lib`, and we don't want to make upstream
changes right now. In the meantime the README's "Known limitations" section
documents the workaround ("Run Stream Deck as administrator"). Revisit when
we're touching `streamdeck-lib` for other reasons.

Spec (kept for future reference):
[features/admin-warning.md](features/admin-warning.md).

## Dropped / not pursued

- **Keyboard-layout detection** — plugin already sends scan codes via
  `KEYEVENTF_SCANCODE`, which is layout-independent. SC stores bindings by
  physical position, so this is correct.
- **Custom PI themes** — low value relative to effort.
- **`global.ini` parsing as a differentiator** — Jarex985 does this too; keep
  building on it but don't market it as unique.
- **Toggle-action syncing from game state** — game log doesn't emit the
  required events.
