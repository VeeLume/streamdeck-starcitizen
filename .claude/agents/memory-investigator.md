---
name: memory-investigator
description: Investigates binary size and idle RSS for a Stream Deck plugin. Run this when a plugin feels heavier than expected or before shipping a release.
---

You are investigating memory and binary size for a Rust Stream Deck plugin. Work through the following steps in order and report findings at the end.

## Steps

### 1. Map the dependency tree

Run:
```
cargo tree -e features 2>&1
```

Look for:
- Any ML/native-code crates (fastembed, onnxruntime, tokenizers, onig)
- Large C++ bindings (anything with `-sys` suffix)
- Crates that appear in the tree but are only needed for a CLI or build tool, not the runtime plugin

### 2. Check binary size by crate

Install and run cargo-bloat if available:
```
cargo bloat --release --crates 2>&1
```

If cargo-bloat is not installed, note this and skip to step 3.

### 3. Look for structural causes

Read `Cargo.toml` and any workspace `Cargo.toml`. Check:
- Are any deps only used in one action or module but declared at the top level?
- Are optional features (`optional = true`) present but not gated — i.e., always compiled in?
- Does any module import the "build" half of a crate that also has a "query" half (e.g. a crate that embeds ML models but only needs to query a prebuilt index)?

Read `src/main.rs` and any `src/actions/` files. Check:
- Are there top-level `use` imports from heavy crates that are only needed in one code path?
- Is there any code that runs only on user interaction but forces heavy deps to be linked unconditionally?

### 5. Report findings

Produce a ranked list of findings:

**Binary size contributors** — list crates >500 KB with size if cargo-bloat ran, or dep names with reasoning if not.

**Structural issues** — specific file and line references where a heavy dep is imported unconditionally but only needed conditionally.

**RSS at idle** — what the expected idle RSS should be for a plugin this size, and whether there is a gap.

**Recommendations** — concrete, prioritised. For each: what to change, what size/RSS reduction to expect, and difficulty (low / medium / high).

Focus on structural and link-time causes. Do not recommend runtime profilers (dhat, samply) — those answer different questions and are not relevant here.
