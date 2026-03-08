# streamdeck-starcitizen

Stream Deck plugin for **starcitizen**, built with Rust and [`streamdeck-lib`](https://github.com/veelume/streamdeck-lib).

## Getting Started

### Install (Users)

1. Download the latest `.streamDeckPlugin` from [Releases](../../releases).
2. Double-click — the Stream Deck app installs it automatically.
3. Find the **starcitizen** category in the action list.

### Build (Developers)

**Prerequisites:** Rust (stable), Node.js (for Elgato CLI).

```sh
# Clone and build
git clone <repo-url>
cd streamdeck-starcitizen
cargo build --release

# Install Elgato CLI (one-time)
npm install -g @elgato/cli
```

#### Initial Setup (from template)

After creating a new project from the template, run the **Initial Setup** VS Code task
(`Ctrl+Shift+P` → `Tasks: Run Task` → `Initial Setup`). It will:

1. Prompt for your plugin name (e.g. `my-plugin`)
2. Replace all `starcitizen` placeholders across every file
3. Rename the `sdPlugin` directory
4. Enable git hooks
5. Link the plugin for local development

### Development Workflow

Use the **Build Plugin** VS Code task (`Ctrl+Shift+B`) which:
1. Stops the running plugin
2. Builds release binary
3. Copies exe into the sdPlugin bundle
4. Restarts the plugin

Or manually:
```sh
cargo build --release
cp target/release/plugin.exe icu.veelume.starcitizen.sdPlugin/bin/icu.veelume.starcitizen.exe
streamdeck restart icu.veelume.starcitizen
```

### Release

This project uses [Conventional Commits](https://www.conventionalcommits.org/) enforced by a
`commit-msg` git hook, and [git-cliff](https://git-cliff.org/) for automated changelogs.

**Commit format:** `<type>[optional scope]: <description>`

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

Examples:
```
feat: add timer action
fix(pi): correct dropdown not updating
feat!: redesign settings storage (breaking)
```

**Release flow:**

```sh
# Install tools (one-time)
cargo install cargo-release git-cliff

# Create a release (bumps version, generates CHANGELOG.md, commits, tags, pushes)
cargo release patch   # or: minor, major
```

GitHub Actions then builds, packs, validates, and creates a GitHub Release with
auto-generated release notes grouped by commit type (features, fixes, etc.).

## Project Structure

```
├── src/
│   ├── main.rs              # Plugin entrypoint
│   ├── actions/             # Stream Deck actions (one file per action)
│   │   ├── mod.rs
│   │   └── example.rs
│   ├── state/               # Shared state stores (Extensions)
│   │   └── mod.rs
│   └── topics.rs            # Pub/sub topic definitions
├── icu.veelume.starcitizen.sdPlugin/
│   ├── manifest.json        # Elgato plugin manifest
│   ├── bin/                 # Compiled binary
│   ├── fonts/               # Custom fonts (for streamdeck-render)
│   ├── images/              # Plugin/action/key icons
│   │   └── README.md        # Image guidelines
│   └── pi/                  # Property Inspector HTML
│       ├── sdpi-components.js
│       ├── README.md         # SDPI component reference
│       └── *.html
├── .github/workflows/       # CI/CD (release with git-cliff notes)
├── .githooks/               # pre-commit, commit-msg, pre-push
├── .claude/hooks/            # Claude Code hooks
├── cliff.toml                # git-cliff changelog config
├── CHANGELOG.md              # Auto-generated changelog
├── CLAUDE.md                 # Guidelines for coding agents
└── README.md                 # This file
```

## Architecture

See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation, coding conventions, and agent guidelines.

## License

MIT OR Apache-2.0
