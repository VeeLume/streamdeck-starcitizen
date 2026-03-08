# Copilot Instructions

Repository-level instructions for GitHub Copilot in VS Code.

For comprehensive architecture documentation, patterns, and coding guidelines,
see [CLAUDE.md](../CLAUDE.md) — it is the authoritative reference for this project.

## Commit Messages

This project enforces **Conventional Commits** via a `commit-msg` git hook.
All commit messages **must** follow this format:

```
<type>[optional scope]: <description>
```

### Types

`feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

### Rules

- The type is **required** and must be one of the types listed above
- The scope is optional, wrapped in parentheses: `feat(pi): ...`
- The description must be lowercase-start, imperative mood, no period at the end
- Use `!` after the type/scope for breaking changes: `feat!: ...`
- The description should be concise (50 chars or less ideal, 72 max)

### Examples

```
feat: add countdown timer action
fix(pi): correct dropdown not updating on refresh
refactor(state): replace RwLock with ArcSwap
docs: update adapter lifecycle in CLAUDE.md
perf: reduce image encoding allocations
build: bump streamdeck-lib to v0.4.4
chore: clean up unused imports
feat!: redesign settings storage model
```

### Bad Examples (will be rejected by hook)

```
Update stuff                     # no type prefix
Fixed the bug                    # no type prefix
feat - add new action            # wrong separator (use colon)
feat:add action                  # missing space after colon
FEAT: add action                 # type must be lowercase
```

## Code Style

- Language: Rust (edition 2024)
- Formatting: `rustfmt` (auto-applied on commit via pre-commit hook)
- Linting: `clippy` (checked on pre-push)
- Error handling: `anyhow` for binaries, `thiserror` for libraries
- Logging: `tracing` crate (`info!`, `debug!`, `warn!`, `error!`)

## Architecture

- Actions in `src/actions/` — one file per action, registered via `ActionFactory::default_of::<T>()`
- State in `src/state/` — shared stores using `Arc<ArcSwap<T>>` or similar
- Topics in `src/topics.rs` — typed pub/sub channels (`TopicId<T>`)
- Adapters in `src/adapters/` — background worker threads
- PI (Property Inspector) HTML in the `sdPlugin/pi/` directory using sdpi-components v4

## Image Generation

- **SVG preferred** for category/action/key icons (write XML directly)
- Category & action icons: monochrome white (#FFFFFF) on transparent background
- When PNG is needed: use `uv run --with pillow` (never `pip install`)
- See CLAUDE.md "Generating Icons & Images" section for full details
