# Game-log telemetry adapter

**Status:** T2, spec'd. Novel feature — neither competitor has it.

## Goal

Tail Star Citizen's active `Game.log` while the game is running and publish
typed topics for the handful of events the log actually emits usefully.
Drives new *informational* actions (QT status, zone indicator, session info)
rather than syncing existing toggle state.

## What the log contains (useful)

Every line is prefixed with an ISO 8601 UTC timestamp:

```
<2026-04-21T23:40:42.708Z> [Notice] <Channel Connection Complete> ...
```

Detection regex for the prefix: `^<\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z>`.

The events we extract:

| Topic                  | Detection substring (contains)                        | Frequency     | Payload                |
| ---------------------- | ----------------------------------------------------- | ------------- | ---------------------- |
| `SESSION_STARTED`      | `AccountLoginCharacterStatus_Character`               | 1 × session   | `{ handle: String }`   |
| `CHANNEL_CONNECTED`    | `Channel Connection Complete`                         | 3 × session   | `{ nickname: String }` |
| `ZONE_LOAD_FINISHED`   | `Loading screen for`                                  | 2–5 × session | `{ zone: String, secs: f32 }` |
| `ARMISTICE_ENTERED`    | `Entering Armistice Zone - Combat Prohibited`         | 10–20 × sess  | `()`                   |
| `ARMISTICE_LEFT`       | `Leaving Armistice Zone - Caution Advised`            | 10–20 × sess  | `()`                   |
| `QT_TARGET_REQUESTED`  | `Player has requested fuel calculation to destination`| 1–5 × session | `{ target: String }`   |
| `QT_ARRIVED`           | `Quantum Drive Arrived`                               | 1–3 × session | `()`                   |
| `SESSION_ENDED`        | `CGame::OnDisconnected` or `SystemQuit`               | 1 × session   | `()`                   |

What's **not** in the log (confirmed, do not try):
- Landing gear / power / shield / cargo / door / seat state — no events.
- Kills, damage, deaths, respawns — no events.
- Runtime keybind changes — no events.
- Ship entered/exited, loadout changes — attachment logs exist but noisy.
- QT drop-out / cancellation — errors only, no dedicated event.
- Explicit "route calculated" confirmation — request is the only signal.

This list is deliberate: speccing events the log actually emits, not the ones
we wish it emitted.

## Architecture

**New adapter:** `GameLogAdapter` with `StartPolicy::OnAppLaunch`.

Lifecycle:
1. `ApplicationDidLaunch(StarCitizen)` → adapter starts.
2. Adapter opens `<active-install>/Game.log` for reading.
3. Adapter tails by seeking to end, polling size every 250 ms, reading new
   bytes, line-splitting, classifying, and publishing topics.
4. `ApplicationDidTerminate(StarCitizen)` → adapter stops; file handle released.

**Why polling, not `notify`:** Windows filesystem notifications on growing
files are unreliable — notifications don't fire on every append, and
ReadDirectoryChangesW sometimes misses events under load. A 250 ms poll of
file metadata (`metadata().len()`) is cheap, reliable, and matches how most
production log tailers (Vector, Filebeat, Fluent Bit) operate on Windows.

**Active-install resolution:** adapter reads `ActiveInstallationState` at
start; if the user switches installations mid-session we don't hot-swap (SC
isn't going to write to a different install's Game.log while running).

**Restart semantics:** each SC launch creates a fresh `Game.log` — SC renames
the previous one into `logbackups/` at shutdown. So "seek to end" at startup
is wrong — we want to read from the beginning to capture the
`AccountLoginCharacterStatus_Character` line at session start.

**Detection:** the adapter checks file size on open. If the file is older
than ~5 minutes and already sizeable, treat as "pre-existing log, resume from
end" (defensive). Otherwise, read from 0.

**Parser:** a single function `classify(line: &str) -> Option<LogEvent>`
doing substring-contains checks + minimal regex for the fields we need.
Order the checks by expected frequency (armistice first, then QT, then
session lifecycle) to minimize CPU on the common path.

## New topics

Defined in `src/topics.rs`:

```rust
pub const SESSION_STARTED: TopicId<SessionStarted> = TopicId::new("starcitizen.session-started");
pub const CHANNEL_CONNECTED: TopicId<ChannelConnected> = TopicId::new("starcitizen.channel-connected");
pub const ZONE_LOAD_FINISHED: TopicId<ZoneLoadFinished> = TopicId::new("starcitizen.zone-load-finished");
pub const ARMISTICE_ENTERED: TopicId<ArmisticeEntered> = TopicId::new("starcitizen.armistice-entered");
pub const ARMISTICE_LEFT: TopicId<ArmisticeLeft> = TopicId::new("starcitizen.armistice-left");
pub const QT_TARGET_REQUESTED: TopicId<QtTargetRequested> = TopicId::new("starcitizen.qt-target-requested");
pub const QT_ARRIVED: TopicId<QtArrived> = TopicId::new("starcitizen.qt-arrived");
pub const SESSION_ENDED: TopicId<SessionEnded> = TopicId::new("starcitizen.session-ended");
```

## New state: `SessionState`

Minimal shared state, populated from topics:

```rust
pub struct SessionState {
    inner: ArcSwap<SessionData>,
}

pub struct SessionData {
    pub handle: Option<String>,          // from SESSION_STARTED
    pub in_armistice: bool,              // flips on ARMISTICE_*
    pub qt_target: Option<String>,       // from QT_TARGET_REQUESTED, cleared on QT_ARRIVED
    pub last_zone: Option<String>,       // from ZONE_LOAD_FINISHED
    pub session_active: bool,            // between SESSION_STARTED and SESSION_ENDED
}
```

## What this enables (later, not in this spec)

Potential new actions that consume these topics:

- **QT status** — shows current target when spooling, empty otherwise.
- **Zone indicator** — shows last loaded zone name.
- **Armistice LED** — simple green/red indicator by `in_armistice`.
- **Session info** — shows handle, can be cleared on SESSION_ENDED.

None of these are part of this spec. This spec only covers the adapter and
topics. Actions that consume them are separate work and go in their own specs
when we commit to each one.

## Out of scope

- Parsing combat / kill / damage events (not emitted).
- Syncing toggle-action state (not emitted).
- Historical log replay from logbackups (noisy, not useful for live UI).
- Multi-session correlation (one active session is enough).

## Test plan

- Unit: `classify` returns correct `LogEvent` variants for each canonical
  line from the log sample. Include a few near-misses (similar prefix but
  different event) to catch regex slop.
- Unit: tail loop correctly resumes from last-read position when file grows.
- Manual: launch SC, watch trace logs for topic publishes matching actual
  in-game events (QT jump, armistice crossing).
- Manual: kill SC mid-session, verify `SESSION_ENDED` fires and adapter
  stops cleanly.

## Risk / unknowns

- **Log format is undocumented.** CIG can change event strings between
  patches. Each topic should log-warn when it's been >N minutes since we
  saw any expected event — doesn't fail, just surfaces "we may be parsing a
  stale format."
- **Log growth rate under extreme load.** A heavy combat scene writes a lot.
  250 ms poll + batched read should handle ~MB/s fine, but worth profiling
  once the adapter is running.

## Step list

1. Add `GameLogAdapter` skeleton with `OnAppLaunch` policy.
2. Implement file tailer (open, read-to-end, seek-and-poll loop, line
   splitter).
3. Implement `classify` with contains-checks + minimal regex.
4. Define new topics in `src/topics.rs`.
5. Add `SessionState` extension, subscribed to the topics.
6. Unit tests against canonical log lines.
7. Manual verification on a real session.
