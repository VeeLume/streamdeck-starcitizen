# Dial / Stream Deck+ support

**Status:** T2, draft. Needs another design pass before implementation — the
UX for choosing CW/CCW/press/tap slots and dial-surface rendering is larger
than first scoped.
**Owner:** plugin.
**Scope:** extend existing actions (Execute + Toggle) with dial handlers rather
than adding a new action type.

## Motivation

Stream Deck+ owners currently have no reason to pick this plugin; both
competitors support dials. The framework (`streamdeck-lib` v0.5.0) already
exposes `dial_down`, `dial_rotate`, `touch_tap` with a `hold` flag for
long-press. Ticks on `DialRotate` are signed `i64` (positive = CW).

## Integration approach

**One action, two controllers.** Extend `ExecuteAction` (and later
`ToggleAction`) to declare both `Keypad` and `Encoder` controllers in the
manifest. The runtime routes `key_*` events or `dial_*`/`touch_tap` events into
the same action struct depending on which surface the user dropped the action
onto.

Rationale: a separate `DialExecuteAction` would duplicate ~half of
`ExecuteAction` (binding lookup, rendering, label processing, style resolution)
and force users to learn two actions for the same concept.

## What a dial-mounted Execute does

A dial has four distinct interactions. We map them as follows:

| Interaction             | Behavior                                             |
| ----------------------- | ---------------------------------------------------- |
| Rotate CW (`ticks > 0`) | Fire the "CW" binding; emit N presses if ticks > 1   |
| Rotate CCW (`ticks < 0`)| Fire the "CCW" binding; emit N presses if ticks > 1  |
| Press (`dial_down`)     | Fire the primary binding (same as keypad press)      |
| Short tap (`hold=false`)| Fire the "tap" binding (or fall back to primary)     |
| Long tap (`hold=true`)  | Fire the "long-tap" binding (or fall back to primary)|

All four slots are optional bindings configured in the PI. Unconfigured slots
do nothing.

## Good dial candidates in SC

From the action-map survey, the actions that naturally pair as CW/CCW:

- `v_ifcs_speed_limiter_increment` / `_decrement` — cruise speed cap
- `v_view_zoom_in` / `_out` — view zoom
- `v_weapon_preset_next` / `_prev` — weapon loadout cycle
- `v_inc_scan_focus_level` / `v_dec_scan_focus_level` — radar zoom
- `v_mfd_interact_cycle_forwards_short` / `_backwards_short` — MFD tabs
- `v_operator_mode_cycle_forward` / `_back` — operator modes
- `pc_conversation_option_up` / `_down` — dialogue menu
- `v_accel_range_increment` / `_decrement` — flight envelope

The PI should make it convenient to pick these pairs (see "PI UX" below).

## Manifest changes

In `manifest.json`, the execute-action entry needs:

```jsonc
{
  "UUID": "icu.veelume.starcitizen.execute-action",
  "Controllers": ["Keypad", "Encoder"],
  "Encoder": {
    "layout": "$B1",        // layout with title + value + bar (TBD)
    "background": "...",
    "Icon": "images/execute-action",
    "TriggerDescription": {
      "Rotate": "Rotate CW/CCW to execute bindings",
      "Push":   "Press to execute primary binding",
      "Touch":  "Tap or long-press for tap bindings"
    }
  },
  ...
}
```

Toggle-action spec mirrors this in a later iteration; not part of the initial
cut.

## Settings schema additions

`ExecuteActionSettings` grows four optional binding refs, each `{mapName,
actionName}`:

```rust
// existing
primary: BindingRef,
hold:    Option<BindingRef>,
double:  Option<BindingRef>,
// new — only meaningful on Encoder controller
rotate_cw:  Option<BindingRef>,
rotate_ccw: Option<BindingRef>,
tap_short:  Option<BindingRef>,
tap_long:   Option<BindingRef>,
```

The Controller type (`Keypad` vs `Encoder`) is known at `will_appear` time
from the event payload; the PI reads it too so it can hide irrelevant fields.

## Rotation → ticks handling

`DialRotate.ticks` is aggregated by the SDK — a quick spin can arrive as
`ticks = 5` in one event. Two modes:

1. **Emit N discrete presses.** Loop `ticks.abs()` times firing the appropriate
   binding. Simple, matches SC's mwheel semantics best.
2. **Rate-limit to one press per event.** Ignore magnitude. Safer for actions
   that mis-behave on rapid repeats.

Default: mode 1. Expose an optional per-dial "max ticks per event" cap in the
PI for the paranoid case. Cap default: 5.

Edge case: many SC "incremental" actions use `hold_toggle` or have `activation
mode = press`. A dial tick should fire a full press/release. Our executor
already does this for keyboard; scroll and mouse paths too.

## Press vs tap precedence

Both `dial_down` (physical press) and `touch_tap` (surface tap) can fire. They
are distinct:
- Press = pushing the encoder in. Analogous to a keypad key press.
- Tap = tapping the touch screen above a specific dial.

We treat them independently: press fires `primary`, short-tap fires `tap_short`
(falls back to primary if unset), long-tap fires `tap_long` (falls back to
primary). A user who only fills `primary` gets the intuitive "any interaction
fires it" behavior.

## Rendering on the dial

The Encoder layout displays an icon + title + optional value bar. Initial
scope:
- Title line: the primary binding's label (same algorithm as keypad).
- Icon: same icon the keypad shows.
- No value bar or feedback-layer state in v1 (no telemetry to drive it yet).
  Leaving room: once the game-log telemetry adapter lands, QT progress / speed
  limiter % could feed a bar.

`set_feedback(ctx, payload)` exists in `streamdeck-lib` — payloads follow
Elgato's documented Encoder layouts. Wrap in a small helper when we need it;
not required for v1.

## PI UX

execute-action.html detects Controller type and renders a dial layout when
`Encoder`. Proposed controls:

- Rotate CW binding (map + action dropdowns)
- Rotate CCW binding
- [button] "Pair as CW/CCW" — opens a picker that lists known increment/
  decrement pairs from the current action map (heuristic: action names ending
  in `_next`/`_prev`, `_in`/`_out`, `_up`/`_down`, `_increment`/`_decrement`,
  `_forwards`/`_backwards`). Fills both slots in one click.
- Press binding (primary)
- Short-tap binding (optional)
- Long-tap binding (optional)
- Max ticks per event (advanced, collapsed section)

The "pair picker" is the one genuine UX improvement over doing everything
manually — it's the difference between building an SC dial profile in 5 minutes
vs 30.

## ToggleAction on dials (later iteration)

Toggle on a dial: press to toggle, rotate could cycle through states (for
multi-state toggles — not currently supported but the roadmap mentions it).
Spec this separately once ToggleAction's state model is decided for >2 states.

## Test plan

- Unit: dial_rotate with ticks=±1/±3/±5 emits correct combo counts.
- Unit: touch_tap with hold=false/true routes to correct settings.
- Manual: on a Stream Deck+ device, verify:
  - CW/CCW both fire their respective bindings.
  - Press fires primary.
  - Touch short/long tap fire correctly, fall back to primary when unset.
  - Setting only primary makes all interactions work.
- Regression: keypad-mounted ExecuteAction keeps working identically.

## Out of scope for v1

- Toggle-action dial support.
- Encoder feedback layer (value bars, icons driven by state).
- Dragging a key action onto a dial converting it automatically.
- Dial-specific style presets.

## Step list

1. Manifest: add `"Controllers": ["Keypad", "Encoder"]` + Encoder block for
   execute-action.
2. Settings: extend `ExecuteActionSettings` with the four new slots.
3. PI: detect Controller at `propertyInspectorDidAppear`, branch layout.
4. PI: implement pair-picker heuristic.
5. Action: implement `dial_down`, `dial_rotate`, `touch_tap` handlers.
6. Rendering: wire `set_feedback` for the dial title; icon reuses keypad path.
7. Tests + manual verification on hardware.
