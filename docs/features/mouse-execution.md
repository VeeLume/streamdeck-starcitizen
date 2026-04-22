# Mouse binding execution

**Status:** Done (mouse-only action fix + tests). The spec is kept for history
and to document the deferred follow-ups. README "Known limitations" covers
the EAC caveat.

## What already works

The mouse execution path is already wired end-to-end:

- Parser — `src/bindings/parser.rs` parses `mouse=`, `<mouse>` children, and
  `mo1_*` strings into `Device::Mouse` bindings.
- Overlay — `src/bindings/overlay.rs` merges user mouse bindings the same as
  keyboard ones.
- Executor — `src/bindings/executor.rs:16-64` accepts `Device::Keyboard |
  Device::Mouse` and calls `binding_to_combo`, which maps `mouse1`–`mouse5`
  via `sc_name_to_mouse_button`, `mwheel_up`/`_down` via `sc_name_to_scroll`,
  and modifiers normally.
- `streamdeck-lib`'s `RawInputExecutor` has `mouse_down`/`mouse_up` for
  `MouseButton::{Left,Right,Middle,X1,X2}` and `scroll(direction, amount)`
  with all four directions.

So a mouse-bound action converts to a correct `InputCombo` once it reaches the
executor. The gap is upstream.

## What's broken

**Key bug:** `src/actions/shared.rs:30-33` — `resolve_combo` filters
`Device::Keyboard` only:

```rust
let kb_binding = action.bindings.iter()
    .find(|b| b.device == Device::Keyboard)?;
```

So when a user picks a mouse-bound action (e.g. FPS `attack1` = `mo1_button1`)
in the PI, the Stream Deck key silently does nothing. This is the real fix.

`describe_action` on the PI-label side already handles both
(`Keyboard | Mouse`), so the label shows correctly — the press just doesn't
fire. Classic "works in the UI, fails silently at runtime."

## Required changes

### 1. Fix `resolve_combo` binding selection

Change the filter to accept mouse bindings and prefer keyboard when both exist.
Rationale: some actions have both (e.g. keyboard default + mouse override);
keyboard is the more reliable choice when it exists because of the EAC mouse
limitation (see "Limitations" below).

```rust
let binding = action.bindings.iter()
    .find(|b| b.device == Device::Keyboard)
    .or_else(|| action.bindings.iter().find(|b| b.device == Device::Mouse))?;
```

### 2. Verify autofill behavior for mouse-defaulted actions

Check: when an action has `Device::Mouse` in defaults and no keyboard binding,
does `autofill` generate a keyboard binding for it? Current behavior in
`autofill.rs:297` — `has_kb` only checks `Device::Keyboard`, so an
action with a mouse-only default is treated as unbound and gets a keyboard
binding generated. That's probably correct behavior: the user gets *both*
options. Confirm with a test case.

### 3. Decide on scroll amount for `mwheel_*` presses

Currently `sc_name_to_scroll` returns `(direction, 1)`. A single Stream Deck
button press emits one scroll tick. That matches SC's behavior for a single
mwheel tick. No change needed for keypad.

For dial rotation (see `dial-action.md`), rotating CW with `ticks=5` should
emit 5 scroll ticks. The dial action handles the `.abs()` looping itself;
`sc_name_to_scroll` stays at 1.

### 4. Document the EAC mouse limitation

When SC is in relative-mouse mode (aiming, moving the mouse to rotate the
view), EAC blocks synthetic mouse-button events. This is not our bug — both
competitors hit it, and Jarex985 documents it explicitly.

Covered in the README "Known limitations" section. A dynamic PI tooltip that
reacts to the currently-selected action resolving to a mouse binding was
considered and **skipped**: the implementation cost (PI-side JS watching the
binding selection and querying the plugin for device info) outweighs the
marginal benefit over a static README note.

## Out of scope

- Horizontal scroll (`mwheel_left`/`_right`) — not used in SC defaults that I
  can see. Add only if we find a case.
- Analog mouse axes (`maxis_x`/`_y`/`_z`) — can't be simulated meaningfully
  with discrete button presses; continue to ignore in executor.
- Absolute mouse move / click at point — not useful for SC bindings.

## Test plan

- Unit: six tests on `pick_executable_binding` — prefers keyboard, falls back
  to mouse when no keyboard, returns None for joystick-only and empty, skips
  joystick in favor of keyboard and mouse respectively. Landed.
- Manual (pending, needs hardware): bind `attack1` (FPS primary fire, mouse1)
  to an Execute key — press out of aim-mode should fire; in aim-mode may not
  (EAC, documented).
- Manual (pending): bind `v_view_zoom_in` (mwheel_up) to an Execute key —
  press should zoom.

## What landed

1. `pick_executable_binding` extracted as pure helper in `src/actions/shared.rs`,
   prefers keyboard, falls back to mouse.
2. `resolve_combo` rewritten to use it (and now finds mouse-only actions).
3. 6 unit tests.
4. README "Known limitations" section covering EAC mouse-in-aim.

## Deferred

- PI tooltip for mouse-bound actions (see §4 above — skipped as
  disproportionate).
- Hardware verification of real mouse-bound actions.
