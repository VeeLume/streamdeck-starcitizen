# Admin-elevation warning

**Status:** Deferred. Clean detection needs an `InputAdapter` failure surface
in `streamdeck-lib` that we don't want to add right now. README's "Known
limitations" section documents the workaround in the meantime.
**Scope:** plugin-side detection and one-shot user notification when
`SendInput` silently fails because the game process runs at higher integrity
than Stream Deck.

## Why this matters

Windows UIPI blocks medium-integrity processes from injecting input into
higher-integrity processes. If Star Citizen is elevated and Stream Deck is
not, `SendInput` returns 0 events injected — no error dialog, no sound, just
nothing happens. New users blame the plugin.

Both competitor plugins document "run Stream Deck as administrator" in their
troubleshooting guides. We want to catch this proactively.

EAC is a separate problem (it blocks synthetic mouse during relative-mouse
mode); that's documented in `mouse-execution.md` and is *not* fixable via
elevation.

## Detection strategy

`streamdeck-lib`'s input executor returns an `ExecuteResult`. For Windows,
`SendInput` returns the number of events successfully injected; zero means
UIPI blocked it (or bad arguments, which we own — they'd reproduce in tests).

Proposed signal: if **every** SendInput call in a given button press returns
zero injected events, and the focused window is Star Citizen, treat it as a
UIPI block.

Implementation note: the plugin doesn't currently inspect `ExecuteResult`
return values from `bus.execute(...)`. We publish the combo and forget. Need
to check whether `InputAdapter` surfaces failures via a topic or return value
we can consume. If not, we either:
1. Lobby upstream for a `INPUT_FAILED` topic emitted by `InputAdapter`.
2. Call `RawInputExecutor` directly from our action for the purpose of the
   check, bypassing the adapter for this one failure-probe case.

Go with (1): it's the architecturally correct place, and any other plugin
benefits too. If that blocks, fall back to (2) on the first press of each
session.

## UX

**First failure:** `show_alert` on the pressed key (existing Stream Deck
visual feedback) + publish a new `INPUT_BLOCKED` topic. Settings action
subscribes to it and renders an "Elevation needed" banner in its PI the next
time it opens. The banner says: "Star Citizen is running elevated but Stream
Deck is not. Right-click the Stream Deck desktop app and choose 'Run as
administrator', then relaunch."

**Subsequent failures in the same session:** no-op. One alert per session is
enough; don't spam.

**Session reset:** on `ApplicationDidTerminate(StarCitizen)`, clear the
"warned" flag.

Store the session-warned flag in a new state extension
`InputDiagnosticsState` (in-memory `ArcSwap<bool>`; no persistence needed).

## Not every zero-events case is UIPI

SendInput can return zero for other reasons — malformed inputs (our bug,
reproducible in tests), or the target window losing focus mid-send. To avoid
false-positive warnings:

- Only warn when the focus check (`GetForegroundWindow` → window title
  contains "Star Citizen") confirms the game is foregrounded.
- Only warn after two consecutive failed presses in the same session. A
  single zero could be a focus race.

The `GetForegroundWindow` check also means we don't warn when the user is
alt-tabbed out — that's legitimate and not an elevation issue.

## Out of scope

- Automatic elevation of the Stream Deck process. We don't ship an elevation
  helper; "relaunch as admin" is the user's action.
- Fixing EAC's mouse limitation. Orthogonal issue.
- Warning when *our* plugin (not Stream Deck itself) is at low integrity —
  we're a child process of the Stream Deck app, inheriting its token.

## Test plan

- Unit: `InputDiagnosticsState` transitions — never-warned → warned →
  cleared on terminate.
- Manual (elevation on): launch SC as admin, Stream Deck as user, press a
  bound key, verify banner appears in Settings PI.
- Manual (elevation off): launch SC as user, Stream Deck as user, verify no
  banner (happy path).
- Manual (alt-tab): press the key while SC is in background; verify no
  banner.

## Step list

1. Decide on the detection surface: check if `InputAdapter` already exposes a
   failure topic. If not, add one upstream or probe via `RawInputExecutor`.
2. Add `InputDiagnosticsState` extension.
3. Publish `INPUT_BLOCKED` topic on confirmed-UIPI press.
4. Settings PI: subscribe, render banner with instructions when set.
5. Clear on `ApplicationDidTerminate`.
6. Manual verification.
