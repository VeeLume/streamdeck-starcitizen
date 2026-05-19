# Audio feedback on button press

**Status:** T2, spec'd.

## Goal

Optional per-action sound effect when a Stream Deck button bound to an
Execute or Toggle action is pressed. User picks a WAV/MP3/OGG file from disk
via the PI; the plugin plays it on a background thread. Silent by default.

Matches parity with both competitor plugins (mhwlng ships 5 bundled WAVs,
Jarex985 supports user WAV/MP3). We don't bundle any sounds — user-supplied
only for v1.

## Crate: `rodio`

- Built on `cpal`; Windows audio-device COM init handled internally.
- Default features enable `symphonia` for MP3/FLAC/OGG/WAV decoding. No
  extra feature flags needed.
- Overlapping presses work: spawn a new `Sink` per play, they mix.
- Binary weight: ~2–3 MB. Acceptable given the current 6 MB baseline.

Alternatives considered: `kira` (overkill — designed for game music with
mixing/tweening), `cpal` directly (too low-level), `awedio` (too immature).

## Architecture: `AudioAdapter`

New adapter in `src/adapters/audio.rs` with `StartPolicy::Eager`. Owns the
`rodio::OutputStream` for the life of the plugin (re-initializing it per
press would add audible latency).

```rust
pub struct AudioAdapter;

impl Adapter for AudioAdapter {
    // Starts a thread that owns `OutputStream` + `OutputStreamHandle`.
    // Receives `PlayRequest { path: PathBuf }` messages from a topic.
    // For each request: open file, decode, append to a new Sink, detach.
}
```

Why an adapter:
- Clean shutdown (adapter lifecycle handles stream teardown).
- Single `OutputStream` ownership — we don't leak streams across presses.
- Actions stay thin — they publish a topic and move on; no blocking in
  `key_down`.

## Topic: `PLAY_SOUND`

```rust
pub const PLAY_SOUND: TopicId<PlaySound> = TopicId::new("starcitizen.play-sound");

pub struct PlaySound {
    pub path: PathBuf,
}
```

Actions fire it on `key_down`. The adapter subscribes and handles playback.

## Per-action settings

`ExecuteActionSettings` and `ToggleActionSettings` grow:

```rust
audio_file: Option<String>,   // absolute path; None = silent
```

PI exposes a file picker. `audio_file` is an absolute path — no plugin-relative
or bundle-relative lookup, no user "sounds folder" convention in v1 (users
place files wherever they want; simpler than a managed folder).

## Handling the rapid-press case

User taps the key three times in a second. Options:

1. **Overlap.** Each press spawns a Sink; they mix naturally. Volume sums
   slightly, but for click-sized samples it's fine.
2. **Drop overlapping.** Skip the second press if the previous is still
   playing. Cleaner audio, worse responsiveness.
3. **Cut-and-restart.** Stop the current sink, start a new one. Feels
   "poppy" due to abrupt cuts.

Default: **overlap** (option 1). It's what rodio does naturally with
multiple Sinks. If users complain we can revisit.

## Error handling

Missing file: log warn once per session per path, publish nothing further.
Decode failure: same. Audio device absent: log warn once at startup; all
subsequent plays silently skip.

We deliberately don't alert the user in the PI when audio fails — it's
supplementary polish, not core functionality. A trace log is enough.

## Memory / perf

- `OutputStream` holds a small OS audio buffer — few hundred KB.
- Each active `Sink` allocates a decode buffer (~100 KB for a short click).
- For a ~1 s WAV file at 44.1 kHz stereo, peak memory per play ~350 KB;
  freed on playback end.
- File I/O happens on the audio thread. For very large user files this
  could briefly block other plays — acceptable given the ~1 s expected
  duration.

## Out of scope for v1

- Bundled click sounds. Users supply their own.
- Volume control (per-action or global). Rodio's Sink has `set_volume`;
  add only if users ask.
- Audio format transcoding, normalization, or trimming.
- Feedback on other events (binding reload, installation change, etc.).
- Audio on dial rotate (each tick firing a sound would be annoying —
  revisit if dial-action lands).

## Test plan

- Unit: `AudioAdapter` thread drains messages and exits cleanly on stop.
- Manual: play a short WAV via key press; confirm audible.
- Manual: tap the key 10 times rapidly; confirm no hang, no crash, audio
  overlaps gracefully.
- Manual: point settings at a non-existent path; confirm no crash, log
  warning once.
- Manual: unplug audio device mid-session; confirm no crash.

## Step list

1. Add `rodio` to `Cargo.toml`.
2. Create `src/adapters/audio.rs` with `AudioAdapter`.
3. Define `PLAY_SOUND` topic.
4. Extend `ExecuteActionSettings` and `ToggleActionSettings` with
   `audio_file`.
5. Wire `key_down` to publish `PLAY_SOUND` when `audio_file` is set.
6. Add file-picker field in execute-action.html and toggle-action.html PIs
   (sdpi-components `<sdpi-file>`).
7. Register `AudioAdapter` in `main.rs`.
8. Unit + manual verification.
