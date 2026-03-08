# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) and other coding agents when working with this repository.

## Project Overview

**streamdeck-starcitizen** is a Rust-based Elgato Stream Deck plugin using the [`streamdeck-lib`](https://github.com/veelume/streamdeck-lib) framework.

**Platform:** Windows-only. The binary runs as a child process of the Stream Deck application, communicating over a local WebSocket.

## Architecture

### Core Concepts

The plugin is built on four pillars:

1. **Actions** — Per-key logic. Each Stream Deck key that uses an action gets its own instance. Lives in `src/actions/`.
2. **Adapters** — Background workers (threads) for I/O, polling, or long-running tasks. Lives in `src/adapters/` (create when needed).
3. **State (Extensions)** — Shared state stores registered at startup, accessed from anywhere via `cx.try_ext::<T>()`. Lives in `src/state/`.
4. **Topics (Pub/Sub)** — Typed event channels that decouple components. Defined in `src/topics.rs`.

### How They Connect

```
┌─────────────┐     publish_t()     ┌──────────┐
│   Adapter    │ ──────────────────► │  Topics  │
│  (bg thread) │                     │ (pub/sub)│
└─────────────┘                     └────┬─────┘
                                         │ on_notify()
┌─────────────┐     publish_t()     ┌────▼─────┐
│   Action     │ ◄──────────────── │  Action   │
│  (per key)   │ ──────────────────► │ (per key) │
└──────┬──────┘                     └──────────┘
       │ cx.try_ext::<T>()
┌──────▼──────┐
│    State     │
│ (Extensions) │
└─────────────┘
```

### Plugin Entrypoint (`src/main.rs`)

```rust
const PLUGIN_ID: &str = "icu.veelume.starcitizen";

fn main() -> anyhow::Result<()> {
    let _guard = init(PLUGIN_ID);               // tracing setup
    let plugin = Plugin::new()
        .add_action(ActionFactory::default_of::<MyAction>())
        .add_adapter(MyAdapter::new())           // optional
        .add_extension(Arc::new(MyStore::new())); // optional
    run_plugin(plugin)
}
```

---

## Actions

Actions live in `src/actions/`, one file per action. Register in `src/actions/mod.rs`.

### Skeleton

```rust
use constcat::concat;
use streamdeck_lib::prelude::*;
use crate::PLUGIN_ID;

#[derive(Default)]
pub struct MyAction { /* per-instance state */ }

impl ActionStatic for MyAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".my-action");
}

impl Action for MyAction {
    fn id(&self) -> &str { Self::ID }

    fn topics(&self) -> &'static [&'static str] {
        &[/* crate::topics::MY_TOPIC.name */]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) { /* init key */ }
    fn did_receive_settings(&mut self, cx: &Context, ev: &DidReceiveSettings) { /* settings changed */ }
    fn key_down(&mut self, cx: &Context, ev: &KeyDown) { /* press */ }
    fn key_up(&mut self, cx: &Context, ev: &KeyUp) { /* release */ }

    fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
        if let Some(payload) = event.downcast(crate::topics::MY_TOPIC) {
            // react
        }
    }
}
```

### Key Action Methods

| Method | When Called |
|--------|-----------|
| `will_appear` | Key becomes visible on the deck |
| `will_disappear` | Key leaves the deck |
| `key_down` / `key_up` | Physical key press/release |
| `dial_down` / `dial_up` / `dial_rotate` | Stream Deck+ encoder |
| `touch_tap` | Stream Deck+ touch strip |
| `did_receive_settings` | PI changed a setting |
| `property_inspector_did_appear` | PI opened |
| `did_receive_sdpi_request` | PI datasource request |
| `did_receive_property_inspector_message` | Raw PI → plugin message |
| `on_notify` | Topic event received |
| `init` / `teardown` | Instance created/destroyed |

### Action ID Convention

Always use `constcat::concat!(PLUGIN_ID, ".action-name")` so the ID matches the manifest UUID.

---

## Adapters

Adapters are background workers. Create in `src/adapters/` when needed.

### Skeleton

```rust
use crossbeam_channel::{Receiver, bounded, select};
use std::sync::Arc;
use streamdeck_lib::prelude::*;

pub struct MyAdapter;

impl AdapterStatic for MyAdapter {
    const NAME: &'static str = "myplugin.my-adapter";
}

impl Adapter for MyAdapter {
    fn name(&self) -> &'static str { Self::NAME }
    fn policy(&self) -> StartPolicy { StartPolicy::Eager }

    fn topics(&self) -> &'static [&'static str] {
        &[/* topics to receive */]
    }

    fn start(&self, cx: &Context, bus: Arc<dyn Bus>, inbox: Receiver<Arc<ErasedTopic>>) -> AdapterResult {
        let (stop_tx, stop_rx) = bounded::<()>(1);

        let join = std::thread::spawn(move || {
            loop {
                select! {
                    recv(inbox) -> msg => match msg {
                        Ok(ev) => { /* handle topic events */ }
                        Err(_) => break,
                    },
                    recv(stop_rx) -> _ => break,
                }
            }
        });

        Ok(AdapterHandle::from_crossbeam(join, stop_tx))
    }
}
```

### Start Policies

| Policy | Behavior |
|--------|----------|
| `Eager` | Starts immediately when plugin starts |
| `OnAppLaunch` | Starts when monitored application launches |
| `Manual` | Started/stopped programmatically |

---

## State (Extensions)

Shared state stores registered at startup. Create in `src/state/`.

### Pattern

```rust
use arc_swap::ArcSwap;
use std::sync::Arc;

pub struct MyStore {
    inner: Arc<ArcSwap<MyData>>,
}

impl MyStore {
    pub fn new() -> Self {
        Self { inner: Arc::new(ArcSwap::from_pointee(MyData::default())) }
    }
    pub fn snapshot(&self) -> Arc<MyData> { self.inner.load_full() }
    pub fn replace(&self, data: MyData) { self.inner.store(Arc::new(data)); }
    pub fn clear(&self) { self.inner.store(Arc::new(MyData::default())); }
}
```

Register: `.add_extension(Arc::new(MyStore::new()))`
Use: `cx.try_ext::<MyStore>().expect("not registered")`

### Recommended backing types

| Type | Use When |
|------|----------|
| `arc_swap::ArcSwap<T>` | Lock-free reads, rare full replacements |
| `parking_lot::RwLock<T>` | Frequent reads, occasional writes |
| `dashmap::DashMap<K,V>` | Concurrent per-key map access |

---

## Topics (Pub/Sub)

Defined in `src/topics.rs`. Typed channels for decoupled communication.

```rust
use streamdeck_lib::prelude::*;

pub const MY_TOPIC: TopicId<MyPayload> = TopicId::new("starcitizen.my-topic");

#[derive(Debug, Clone)]
pub struct MyPayload { pub value: String }
```

**Publish:** `cx.bus().publish_t(MY_TOPIC, payload);`

**Subscribe (action):**
```rust
fn topics(&self) -> &'static [&'static str] { &[MY_TOPIC.name] }
fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
    if let Some(p) = event.downcast(MY_TOPIC) { /* ... */ }
}
```

**Subscribe (adapter):** declare in `fn topics()`, received via `inbox` channel.

### Targeting

| Method | Reaches |
|--------|---------|
| `bus.publish_t(topic, val)` | All actions + adapters subscribed to topic |
| `bus.action_notify_all_t(topic, val)` | All action instances |
| `bus.action_notify_context_t(ctx, topic, val)` | Specific key instance |
| `bus.action_notify_id_t(action_id, topic, val)` | All instances of one action type |
| `bus.adapters_notify_all_t(topic, val)` | All adapters |

---

## Context (`cx: &Context`)

Available in all action methods and adapter start.

| Method | Returns | Purpose |
|--------|---------|---------|
| `cx.sd()` | `&SdClient` | Send commands to Stream Deck |
| `cx.bus()` | `Arc<dyn Bus>` | Publish topics, notify actions/adapters |
| `cx.globals()` | `GlobalSettings` | Read/write plugin-wide settings |
| `cx.try_ext::<T>()` | `Option<Arc<T>>` | Access registered extensions |
| `cx.sdpi()` | `Sdpi` | Reply to PI datasource requests |

### SdClient Methods

```rust
cx.sd().set_title(ctx_id, Some("Title".into()), None, None);
cx.sd().set_image(ctx_id, Some("imgs/icon".into()), None, None);
cx.sd().set_image_b64(ctx_id, base64_string);
cx.sd().show_ok(ctx_id);
cx.sd().show_alert(ctx_id);
cx.sd().set_settings(ctx_id, settings_map);
cx.sd().set_global_settings(settings_map);
cx.sd().send_to_property_inspector(ctx_id, payload);
cx.sd().open_url("https://example.com");
```

### Global Settings

```rust
let globals = cx.globals();
globals.set("key", serde_json::Value::String("value".into()));
let val = globals.get("key");
globals.with_mut(|map| { map.insert("k".into(), json!("v")); });
```

---

## Property Inspector (SDPI)

HTML files in `icu.veelume.starcitizen.sdPlugin/pi/`. Uses sdpi-components v4 (local JS file, no CDN).

See `pi/README.md` for the full component reference.

### Minimal template

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8" />
    <script src="sdpi-components.js"></script>
</head>
<body>
    <sdpi-item label="My Setting">
        <sdpi-textfield setting="mySetting" placeholder="Enter value"></sdpi-textfield>
    </sdpi-item>
</body>
</html>
```

### Dynamic datasource

PI side:
```html
<sdpi-select setting="actionId" datasource="getActions" hot-reload></sdpi-select>
```

Rust side:
```rust
fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
    if req.event == "getActions" {
        cx.sdpi().reply(req, vec![
            DataSourceResultItem::Item(DataSourceItem {
                disabled: None,
                label: Some("Action A".into()),
                value: "a".into(),
            }),
        ]);
    }
}
```

---

## Manifest (`manifest.json`)

Located at `icu.veelume.starcitizen.sdPlugin/manifest.json`.

Key fields:
- `UUID`: Must match `PLUGIN_ID` in `main.rs`
- `CodePath`: Relative path to the exe inside the bundle
- `Actions[].UUID`: Must match `ActionStatic::ID` for each action
- `Actions[].PropertyInspectorPath`: Relative path to the PI HTML

When adding a new action:
1. Create the Rust action in `src/actions/`
2. Register in `mod.rs` and `main.rs`
3. Add entry to `manifest.json` Actions array
4. Create PI HTML in `pi/`

---

## Images

See `images/README.md` for size/format guidelines.

| Type | Size | Format | Color |
|------|------|--------|-------|
| Plugin Icon | 256×256 / 512×512 | PNG | Full color |
| Category Icon | 28×28 / 56×56 | SVG | Mono white #FFF |
| Action Icon | 20×20 / 40×40 | SVG | Mono white #FFF |
| Key Icon | 72×72 / 144×144 | SVG/PNG | Any |

For programmatic rendering (custom fonts, dynamic text), use `streamdeck-render`:
```rust
let mut canvas = Canvas::key_icon(); // 144×144
let lines = wrap_text(&font, 28.0, "Label", &WrapOptions::default());
canvas.draw_text(&lines, &TextOptions::new(font, 28.0).color(Color::WHITE))?;
cx.sd().set_image_b64(ctx_id, canvas.finish().to_base64()?);
```

---

## Generating Icons & Images (for coding agents)

This section explains how to create SVG and PNG icons for the plugin.
**Read this before generating any images.**

### Decision: SVG vs PNG

| Need | Use |
|------|-----|
| Category icon (28×28) | SVG — write by hand |
| Action icon (20×20) | SVG — write by hand |
| Key icon (72×72 / 144×144) | SVG preferred, PNG if raster needed |
| Plugin icon (256×256 / 512×512) | PNG (only image type that must be PNG) |
| Runtime dynamic icon | `streamdeck-render` crate (Rust, at runtime) |

### Option 1: Write SVG directly (preferred for icons)

SVG is the best choice for most Stream Deck icons. Write the XML directly — no tooling needed.

**Category & Action icons** must be monochrome white (#FFFFFF) on transparent background:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
  <circle cx="10" cy="10" r="7" fill="none" stroke="#FFFFFF" stroke-width="1.5"/>
  <line x1="10" y1="6" x2="10" y2="14" stroke="#FFFFFF" stroke-width="1.5" stroke-linecap="round"/>
  <line x1="6" y1="10" x2="14" y2="10" stroke="#FFFFFF" stroke-width="1.5" stroke-linecap="round"/>
</svg>
```

**Key icons** can use full color:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">
  <rect width="144" height="144" rx="12" fill="#1a1a2e"/>
  <text x="72" y="80" text-anchor="middle" fill="#FFFFFF"
        font-family="Arial" font-size="24" font-weight="bold">LABEL</text>
</svg>
```

**SVG tips:**
- Always include `xmlns="http://www.w3.org/2000/svg"`
- Set `width`, `height`, and `viewBox` to match the target size
- Use `stroke-linecap="round"` for clean line endings
- Keep shapes simple — these render at tiny sizes
- Minimum stroke width: 1.5px for 20×20, 2px for 28×28
- Test: open the SVG in a browser and zoom out to actual size

### Option 2: Python + Pillow via uv (for PNG generation)

When you need raster PNGs (e.g., the plugin icon, or complex generated graphics),
use Python with Pillow. **Always use `uv` to run Python with dependencies** — do NOT
use `pip install` or assume Pillow is installed.

**Inline one-liner:**
```bash
echo '
from PIL import Image, ImageDraw
img = Image.new("RGBA", (256, 256), (45, 45, 45, 255))
draw = ImageDraw.Draw(img)
draw.rounded_rectangle([8, 8, 248, 248], radius=24, outline=(96, 96, 96), width=4)
draw.line([(128, 80), (128, 176)], fill="white", width=12)
draw.line([(80, 128), (176, 128)], fill="white", width=12)
img.save("plugin.png")
print("Created plugin.png")
' | uv run --with pillow -
```

**Script file:**
```bash
uv run --with pillow generate_icon.py
```

**With multiple dependencies:**
```bash
uv run --with pillow --with cairosvg my_script.py
```

**Key rules:**
- ALWAYS use `uv run --with pillow` — never `pip install pillow`
- For inline scripts: `echo '...' | uv run --with pillow -`
- For script files: `uv run --with pillow script.py`
- See https://docs.astral.sh/uv/guides/scripts/#running-a-script-with-dependencies

**Common Pillow patterns for Stream Deck icons:**

```python
from PIL import Image, ImageDraw, ImageFont

# Transparent background (for action/category icons)
img = Image.new("RGBA", (40, 40), (0, 0, 0, 0))

# Solid background (for key icons)
img = Image.new("RGBA", (144, 144), (26, 26, 46, 255))

# Drawing
draw = ImageDraw.Draw(img)
draw.rounded_rectangle([4, 4, 140, 140], radius=12, fill=(30, 30, 50), outline=(100, 100, 255), width=3)
draw.ellipse([50, 50, 94, 94], outline="white", width=2)
draw.text((72, 72), "Hi", fill="white", anchor="mm")  # centered text

# With custom font (if available)
font = ImageFont.truetype("fonts/MyFont.ttf", 24)
draw.text((72, 72), "Hi", fill="white", font=font, anchor="mm")

# Save
img.save("icon.png")

# Create @2x variant
img_2x = img.resize((512, 512), Image.LANCZOS)
img_2x.save("icon@2x.png")
```

### Option 3: SVG → PNG conversion via Python

If you have an SVG and need a PNG (e.g., for the plugin icon):

```bash
echo '
import cairosvg
cairosvg.svg2png(url="images/plugin.svg", write_to="images/plugin.png", output_width=256, output_height=256)
cairosvg.svg2png(url="images/plugin.svg", write_to="images/plugin@2x.png", output_width=512, output_height=512)
print("Converted SVG to PNG")
' | uv run --with cairosvg -
```

### Option 4: streamdeck-render (Rust, runtime only)

For dynamic key icons rendered at runtime (not build-time assets), use the
`streamdeck-render` crate. This is for icons that change based on plugin state.

See the streamdeck-render section above and the crate's README for details.

### What NOT to do

- **Do NOT use `pip install`** — always use `uv run --with <package>`
- **Do NOT assume ImageMagick/Inkscape are installed** — use `uv` + Python instead
- **Do NOT generate PNG for category/action icons** — use SVG (it scales perfectly)
- **Do NOT use colored icons for category/action** — must be monochrome white #FFFFFF
- **Do NOT forget `xmlns`** in SVG — the file will be invalid without it
- **Do NOT create overly complex SVGs** — these render at 20-28px, keep it simple

---

## Development Commands

```bash
cargo build --release                    # Build
cargo fmt --all                          # Format
cargo clippy --all-targets --all-features # Lint
streamdeck link icu.veelume.starcitizen.sdPlugin  # Link for dev (once)
streamdeck restart icu.veelume.starcitizen         # Restart plugin
streamdeck validate icu.veelume.starcitizen.sdPlugin # Validate manifest
git-cliff --output CHANGELOG.md          # Preview changelog
cargo release patch                      # Release (bump, changelog, tag, push)
```

## Git Hooks

Enable with: `git config core.hooksPath .githooks`

- **pre-commit:** `cargo fmt --all` + stage changes
- **commit-msg:** Enforces [Conventional Commits](https://www.conventionalcommits.org/) format
- **pre-push:** fmt check, clippy, cargo check, streamdeck validate

## Commit Convention

All commits **must** follow the Conventional Commits format:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

**Examples:**
```
feat: add countdown timer action
fix(pi): correct color picker default value
refactor(state): switch to ArcSwap for lock-free reads
docs: add adapter lifecycle diagram to CLAUDE.md
feat!: redesign settings model (breaking change)
```

The `commit-msg` hook rejects non-conforming messages. This is required because
`git-cliff` uses these prefixes to auto-generate grouped changelogs.

## Release Flow

```bash
# One-time tool install
cargo install cargo-release git-cliff

# Release (bumps version, generates CHANGELOG.md, commits, tags, pushes)
cargo release patch   # or: minor, major
```

**What happens:**
1. `cargo-release` bumps the version in `Cargo.toml`
2. `git-cliff` regenerates `CHANGELOG.md` (via `pre-release-hook` in `release.toml`)
3. A commit `chore(release): <version>` is created and tagged `v<version>`
4. Tag is pushed → GitHub Actions builds, packs, and creates a GitHub Release
5. Release notes are generated by `git-cliff --latest` (grouped by commit type)

**Do not** edit `CHANGELOG.md` manually — it is overwritten on each release.

## Conventions

- **Commits:** Conventional Commits (enforced by hook)
- **Error handling:** `anyhow` for the binary, `thiserror` if extracting a library crate
- **Logging:** `tracing` macros (`info!`, `debug!`, `warn!`, `error!`)
- **Action IDs:** `constcat::concat!(PLUGIN_ID, ".action-name")`
- **Adapter names:** `"pluginname.adapter-name"`
- **Topic names:** `"pluginname.topic-name"`
- **Settings keys:** camelCase (matches JS convention in PI)

## External Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `streamdeck-lib` | SD protocol, actions, adapters, bus | Git dep, pinned to tag |
| `streamdeck-render` | PNG icon rendering with custom fonts | Git dep, optional |
| `constcat` | Compile-time string concat for action IDs | |
| `anyhow` | Error handling | |
| `serde` / `serde_json` | Settings serialization | |
| `tracing` | Structured logging | |

### Reduction checklist

| Concern | Fix |
|---------|-----|
| Heap grows over time | Use `dhat` to find the retaining allocation; check for unbounded caches or `Vec` that never shrinks |
| High idle CPU | Use `tokio-console` to find tasks with high poll counts; check for tight polling loops |
| Large baseline RSS | Strip release binary (`strip = "symbols"` is set); consider `jemalloc` → `mimalloc` swap if needed |
| Slow key response | `tokio-console` → sort by max-poll-duration; look for blocking calls inside async tasks |
| Big allocations at startup | `profiling::log_memory("before-load")` + `("after-load")` to isolate; use `dhat` |

### Binary size

The release profile is already tuned (`lto = "thin"`, `codegen-units = 1`,
`strip = "symbols"`). Check the size after a release build:

```powershell
(Get-Item target/release/plugin.exe).Length / 1MB
```

If the binary is still larger than expected, try `cargo bloat --release` to find
large dependencies.

```bash
cargo install cargo-bloat
cargo bloat --release --crates   # size per crate
cargo bloat --release -n 20      # top 20 functions by size
```

---

## Workspace Conversion

For larger plugins, convert to a workspace:

```toml
# Cargo.toml (root)
[workspace]
members = ["crates/core", "crates/plugin", "crates/cli"]
resolver = "2"

[workspace.dependencies]
streamdeck-lib = { git = "...", tag = "v0.5.0" }
# ... shared deps
```

Split domain logic into `crates/core` (library) and keep plugin-specific code in `crates/plugin`.
