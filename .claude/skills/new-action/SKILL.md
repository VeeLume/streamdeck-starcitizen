---
name: new-action
description: Scaffold a new Stream Deck action — Rust file, mod.rs registration, manifest.json entry, and PI HTML.
---

Create a new Stream Deck action named $ARGUMENTS. Follow the patterns in CLAUDE.md exactly.

Steps:
1. Create `src/actions/$NAME.rs` using the Action skeleton from CLAUDE.md.
   - `ActionStatic::ID` must be `concat!(PLUGIN_ID, ".$NAME")`
   - Include all lifecycle method stubs with doc comments
2. Register in `src/actions/mod.rs` — add `pub mod $NAME;`
3. Register in `src/main.rs` — add `.add_action(ActionFactory::default_of::<${PascalCase}Action>())`
4. Add to `manifest.json` Actions array — UUID = `icu.veelume.starcitizen.$NAME`, PropertyInspectorPath = `pi/$NAME.html`
5. Create `icu.veelume.starcitizen.sdPlugin/pi/$NAME.html` from the minimal PI template in CLAUDE.md

Use $ARGUMENTS as the action name (kebab-case, e.g. `countdown-timer`).
