//! Generate `toggle-groups.toml` from a Star Citizen installation.
//!
//! Identifies toggle actions using two XML signals:
//! - `<states>` child elements (explicit on/off, locked/unlocked, etc.)
//! - `activationMode="smart_toggle"`
//! - Name contains "toggle" (permissive fallback)
//!
//! Then finds enable/disable siblings via token overlap within the same actionmap.
//!
//! Usage:
//!   toggle-groups-gen.exe                    # auto-discover LIVE installation
//!   toggle-groups-gen.exe "path/to/Data.p4k" # explicit path

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use streamdeck_starcitizen::bindings::HIDDEN_ACTION_MAPS;
use streamdeck_starcitizen::bindings::model::{GameAction, ParsedBindings};
use streamdeck_starcitizen::discovery::{self, Channel};

// ── Toggle group model ──────────────────────────────────────────────────────────

struct ToggleGroup {
    map: String,
    toggle: String,
    on: Option<String>,
    off: Option<String>,
    /// All sibling candidates: (name, score, role).
    /// Role is "on" or "off" based on classify_sibling or state-keyword inference.
    candidates: Vec<(String, f64, &'static str)>,
}

// ── Heuristics ──────────────────────────────────────────────────────────────────

/// Noise words stripped during tokenization.
/// Note: `short` and `long` are NOT noise — they distinguish activation variants
/// (e.g. `v_auto_targeting_toggle_short` vs `_long`).
const NOISE: &[&str] = &[
    "v", "p", "toggle", "enable", "disable", "set", "start", "stop",
];

fn tokenize(name: &str) -> HashSet<String> {
    name.to_lowercase()
        .split('_')
        .filter(|p| p.len() > 1 && !NOISE.contains(p))
        .map(|s| s.to_string())
        .collect()
}

fn is_toggle_action(action: &GameAction) -> bool {
    !action.states.is_empty()
        || action
            .activation_mode
            .as_deref()
            .is_some_and(|m| m == "smart_toggle")
        || action.name.contains("toggle")
}

/// Rough stem: "opened" → "open", "retracted" → "retract", etc.
fn stem_state(name: &str) -> String {
    let s = name.to_lowercase();
    if s.ends_with("ed") && s.len() > 4 {
        let base = &s[..s.len() - 2];
        // Handle double consonant: "equipped" → "equip"
        if base.ends_with("pp") {
            return base[..base.len() - 1].to_string();
        }
        return base.to_string();
    }
    s
}

/// Derive state keywords from a toggle's states (e.g., ["open", "close"]).
/// Returns None if states are trivial (on/off) or absent.
fn state_keywords(action: &GameAction) -> Option<HashSet<String>> {
    if action.states.is_empty() {
        return None;
    }
    let keywords: HashSet<String> = action
        .states
        .iter()
        .map(|s| stem_state(&s.name))
        .filter(|s| s != "on" && s != "off")
        .collect();
    if keywords.is_empty() {
        None
    } else {
        Some(keywords)
    }
}

fn find_siblings(
    toggle: &GameAction,
    all_actions: &[GameAction],
) -> Vec<(String, f64, &'static str)> {
    let toggle_tokens = tokenize(&toggle.name);
    if toggle_tokens.is_empty() {
        return vec![];
    }

    let keywords = state_keywords(toggle);

    // Build a map from stemmed state name → role for state-keyword inference.
    // First state = "on", second state = "off" (matches SC convention).
    let state_role_map: Vec<(String, &'static str)> = toggle
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (stem_state(&s.name), if i == 0 { "on" } else { "off" }))
        .collect();

    let mut siblings = Vec::new();

    for other in all_actions {
        if other.name == toggle.name || is_toggle_action(other) {
            continue;
        }
        let other_tokens = tokenize(&other.name);
        if other_tokens.is_empty() {
            continue;
        }

        if let Some(ref kw) = keywords {
            let other_lower = other.name.to_lowercase();
            let has_keyword = kw.iter().any(|k| other_lower.contains(k.as_str()));
            let overlap: HashSet<_> = toggle_tokens.intersection(&other_tokens).collect();
            let enough = overlap.len() as f64 >= toggle_tokens.len() as f64 * 0.5;
            if !(has_keyword && enough) {
                continue;
            }
        } else if !toggle_tokens.is_subset(&other_tokens) {
            continue;
        }

        let overlap: HashSet<_> = toggle_tokens.intersection(&other_tokens).collect();
        let union: HashSet<_> = toggle_tokens.union(&other_tokens).collect();
        let score = overlap.len() as f64 / union.len() as f64;

        // Determine role: try classify_sibling first, then state-keyword inference
        let role = classify_sibling(&other.name).unwrap_or_else(|| {
            let other_lower = other.name.to_lowercase();
            for (stemmed, role) in &state_role_map {
                if other_lower.contains(stemmed.as_str()) {
                    return role;
                }
            }
            "on" // default guess
        });

        siblings.push((other.name.to_string(), score, role));
    }

    siblings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    siblings
}

/// On/off suffix words used to strip from action names when reverse-searching.
const ON_OFF_SUFFIXES: &[&str] = &[
    "on", "off", "enable", "disable", "open", "close", "lock", "unlock", "deploy", "retract",
];

fn classify_sibling(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    // Check for "on"-like suffixes
    if lower.ends_with("_on")
        || lower.contains("_enable")
        || lower.ends_with("_open")
        || lower.ends_with("_unlock")
        || lower.ends_with("_deploy")
    {
        return Some("on");
    }
    // Check for "off"-like suffixes
    if lower.ends_with("_off")
        || lower.contains("_disable")
        || lower.ends_with("_close")
        || lower.ends_with("_lock")
        || lower.ends_with("_retract")
    {
        return Some("off");
    }
    None
}

/// Returns true if this action is a `_long` or `_hold` variant and the
/// base / `_short` version exists in the same map.
fn is_redundant_variant(name: &str, map_action_names: &HashSet<&str>) -> bool {
    if name.ends_with("_long") {
        let base = name.strip_suffix("_long").unwrap();
        let short = format!("{base}_short");
        return map_action_names.contains(short.as_str()) || map_action_names.contains(base);
    }
    if name.ends_with("_hold") {
        let base = name.strip_suffix("_hold").unwrap();
        return map_action_names.contains(base);
    }
    false
}

// ── Group discovery ─────────────────────────────────────────────────────────────

fn discover_groups(bindings: &ParsedBindings) -> Vec<ToggleGroup> {
    let mut groups = Vec::new();
    let mut claimed: HashSet<String> = HashSet::new();

    // Pass 1: find toggle actions and their siblings
    for map in &bindings.action_maps {
        if HIDDEN_ACTION_MAPS.contains(&map.name.as_ref()) {
            continue;
        }

        // Collect toggle names in this map so we can skip _long when _short exists
        let map_action_names: HashSet<&str> = map.actions.iter().map(|a| a.name.as_ref()).collect();

        for action in &map.actions {
            if !is_toggle_action(action) {
                continue;
            }

            // Skip activation-mode variants when a base or _short variant exists.
            // _long → prefer _short; _hold → prefer the base (without _hold).
            if action.name.ends_with("_long") {
                let base = action.name.strip_suffix("_long").unwrap();
                let short_name = format!("{base}_short");
                if map_action_names.contains(short_name.as_str()) || map_action_names.contains(base)
                {
                    continue;
                }
            }
            if action.name.ends_with("_hold") {
                let base = action.name.strip_suffix("_hold").unwrap();
                if map_action_names.contains(base) {
                    continue;
                }
            }

            let siblings = find_siblings(action, &map.actions);

            let mut on_action = None;
            let mut off_action = None;

            for (sib_name, _score, role) in &siblings {
                match (*role, on_action.is_none(), off_action.is_none()) {
                    ("on", true, _) => {
                        on_action = Some(sib_name.clone());
                        claimed.insert(sib_name.clone());
                    }
                    ("off", _, true) => {
                        off_action = Some(sib_name.clone());
                        claimed.insert(sib_name.clone());
                    }
                    _ => {}
                }
            }

            claimed.insert(action.name.to_string());
            groups.push(ToggleGroup {
                map: map.name.to_string(),
                toggle: action.name.to_string(),
                on: on_action,
                off: off_action,
                candidates: siblings,
            });
        }
    }

    // Pass 2: find orphan on/off pairs and reverse-search for their toggle
    for map in &bindings.action_maps {
        if HIDDEN_ACTION_MAPS.contains(&map.name.as_ref()) {
            continue;
        }

        let map_action_names: HashSet<&str> = map.actions.iter().map(|a| a.name.as_ref()).collect();

        let on_actions: Vec<_> = map
            .actions
            .iter()
            .filter(|a| {
                !claimed.contains(a.name.as_ref())
                    && classify_sibling(&a.name) == Some("on")
                    && !is_redundant_variant(&a.name, &map_action_names)
            })
            .collect();

        let off_actions: Vec<_> = map
            .actions
            .iter()
            .filter(|a| {
                !claimed.contains(a.name.as_ref())
                    && classify_sibling(&a.name) == Some("off")
                    && !is_redundant_variant(&a.name, &map_action_names)
            })
            .collect();

        for on_a in &on_actions {
            let on_tokens = tokenize(&on_a.name);
            let base_tokens: HashSet<_> = on_tokens
                .iter()
                .filter(|t| !ON_OFF_SUFFIXES.contains(&t.as_str()))
                .cloned()
                .collect();
            if base_tokens.is_empty() {
                continue;
            }

            // Find matching off action
            let mut best_off: Option<&GameAction> = None;
            let mut best_score = 0.0;
            for off_a in &off_actions {
                let off_tokens = tokenize(&off_a.name);
                let off_base: HashSet<_> = off_tokens
                    .iter()
                    .filter(|t| !ON_OFF_SUFFIXES.contains(&t.as_str()))
                    .cloned()
                    .collect();
                if off_base == base_tokens {
                    best_off = Some(off_a);
                    best_score = 1.0;
                    break;
                }
                let overlap: HashSet<_> = base_tokens.intersection(&off_base).collect();
                let union: HashSet<_> = base_tokens.union(&off_base).collect();
                let score = overlap.len() as f64 / union.len() as f64;
                if score > best_score {
                    best_score = score;
                    best_off = Some(off_a);
                }
            }

            if best_score < 0.8 {
                continue;
            }
            let off_a = match best_off {
                Some(a) => a,
                None => continue,
            };

            // Find toggle candidate: action whose tokens == base_tokens
            let toggle_candidate = map
                .actions
                .iter()
                .filter(|a| !claimed.contains(a.name.as_ref()))
                .filter(|a| a.name != on_a.name && a.name != off_a.name)
                .find(|a| {
                    let t = tokenize(&a.name);
                    t == base_tokens
                        || (base_tokens.is_subset(&t)
                            && t.difference(&base_tokens)
                                .all(|extra| extra == "cycle" || extra == "toggle"))
                });

            let toggle_name = toggle_candidate
                .map(|c| c.name.to_string())
                .unwrap_or_default();

            if !toggle_name.is_empty() {
                claimed.insert(toggle_name.clone());
            }
            claimed.insert(on_a.name.to_string());
            claimed.insert(off_a.name.to_string());

            // If no toggle found, use the on action name as a placeholder group key
            let toggle = if toggle_name.is_empty() {
                // No toggle found — still create the group with just on/off
                format!("# no toggle found for {}", on_a.name)
            } else {
                toggle_name
            };

            groups.push(ToggleGroup {
                map: map.name.to_string(),
                toggle,
                on: Some(on_a.name.to_string()),
                off: Some(off_a.name.to_string()),
                candidates: vec![],
            });
        }
    }

    groups
}

// ── TOML output ─────────────────────────────────────────────────────────────────

fn write_toml(groups: &[ToggleGroup], out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "# Toggle group definitions for Star Citizen.")?;
    writeln!(
        out,
        "# Generated by toggle-groups-gen.exe — review and hand-curate as needed."
    )?;
    writeln!(
        out,
        "# Re-run after SC updates to pick up new toggle actions."
    )?;
    writeln!(out)?;

    let mut current_map = "";
    for g in groups {
        if g.map != current_map {
            if !current_map.is_empty() {
                writeln!(out)?;
            }
            writeln!(out, "# ── {map} ──", map = g.map)?;
            writeln!(out)?;
            current_map = &g.map;
        }

        if g.toggle.starts_with('#') {
            // Comment-only entry (no toggle found)
            writeln!(out, "{}", g.toggle)?;
            if let Some(ref on) = g.on {
                writeln!(out, "# on = \"{}\"", on)?;
            }
            if let Some(ref off) = g.off {
                writeln!(out, "# off = \"{}\"", off)?;
            }
            writeln!(out)?;
            continue;
        }

        writeln!(out, "[[group]]")?;
        writeln!(out, "map = \"{}\"", g.map)?;
        writeln!(out, "toggle = \"{}\"", g.toggle)?;
        if let Some(ref on) = g.on {
            writeln!(out, "on = \"{}\"", on)?;
        }
        if let Some(ref off) = g.off {
            writeln!(out, "off = \"{}\"", off)?;
        }
        // Show sibling candidates as commented-out on/off lines (helps curation)
        if !g.candidates.is_empty() {
            let picked: HashSet<&str> = [
                g.on.as_deref().unwrap_or(""),
                g.off.as_deref().unwrap_or(""),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
            let unpicked: Vec<_> = g
                .candidates
                .iter()
                .filter(|(name, _, _)| !picked.contains(name.as_str()))
                .collect();
            if !unpicked.is_empty() {
                for (name, score, role) in &unpicked {
                    writeln!(out, "# {role} = \"{name}\" # {:.0}%", score * 100.0)?;
                }
            }
        }
        writeln!(out)?;
    }

    Ok(())
}

// ── Diff against existing TOML ──────────────────────────────────────────────────

/// TOML schema for reading an existing toggle-groups.toml.
#[derive(Debug, Clone, serde::Deserialize)]
struct ExistingGroup {
    map: String,
    toggle: String,
    on: Option<String>,
    off: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ExistingFile {
    #[serde(default)]
    group: Vec<ExistingGroup>,
}

fn load_existing(path: &std::path::Path) -> Vec<ExistingGroup> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return vec![];
    };
    toml::from_str::<ExistingFile>(&content)
        .map(|f| f.group)
        .unwrap_or_default()
}

struct Diff {
    added: Vec<ToggleGroup>,
    removed: Vec<ExistingGroup>,
    changed: Vec<ChangedGroup>,
}

struct ChangedGroup {
    map: String,
    toggle: String,
    old_on: Option<String>,
    old_off: Option<String>,
    new_on: Option<String>,
    new_off: Option<String>,
    candidates: Vec<(String, f64, &'static str)>,
}

fn compute_diff(existing: &[ExistingGroup], discovered: &[ToggleGroup]) -> Diff {
    let existing_keys: HashMap<String, &ExistingGroup> = existing
        .iter()
        .map(|g| (format!("{}.{}", g.map, g.toggle), g))
        .collect();

    let discovered_keys: HashMap<String, &ToggleGroup> = discovered
        .iter()
        .filter(|g| !g.toggle.starts_with('#'))
        .map(|g| (format!("{}.{}", g.map, g.toggle), g))
        .collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();

    for (key, dg) in &discovered_keys {
        match existing_keys.get(key) {
            None => {
                added.push(ToggleGroup {
                    map: dg.map.clone(),
                    toggle: dg.toggle.clone(),
                    on: dg.on.clone(),
                    off: dg.off.clone(),
                    candidates: dg.candidates.clone(),
                });
            }
            Some(eg) => {
                // Check if auto-detected siblings differ
                let on_changed = dg.on.is_some() && dg.on != eg.on && eg.on.is_none();
                let off_changed = dg.off.is_some() && dg.off != eg.off && eg.off.is_none();
                if on_changed || off_changed {
                    changed.push(ChangedGroup {
                        map: dg.map.clone(),
                        toggle: dg.toggle.clone(),
                        old_on: eg.on.clone(),
                        old_off: eg.off.clone(),
                        new_on: dg.on.clone(),
                        new_off: dg.off.clone(),
                        candidates: dg.candidates.clone(),
                    });
                }
            }
        }
    }

    let removed: Vec<_> = existing
        .iter()
        .filter(|eg| {
            let key = format!("{}.{}", eg.map, eg.toggle);
            !discovered_keys.contains_key(&key)
        })
        .cloned()
        .collect();

    Diff {
        added,
        removed,
        changed,
    }
}

fn write_diff(diff: &Diff, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "# Toggle Groups Diff")?;
    writeln!(
        out,
        "# Generated by toggle-groups-gen.exe against existing toggle-groups.toml"
    )?;
    writeln!(
        out,
        "# Review and apply changes to toggle-groups.toml as needed."
    )?;
    writeln!(out)?;

    if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
        writeln!(
            out,
            "# No changes detected — toggle-groups.toml is up to date."
        )?;
        return Ok(());
    }

    // Added
    if !diff.added.is_empty() {
        writeln!(
            out,
            "# ══ NEW ({count}) — found in SC but not in existing TOML ══",
            count = diff.added.len()
        )?;
        writeln!(
            out,
            "# Copy these [[group]] entries into toggle-groups.toml if desired."
        )?;
        writeln!(out)?;

        for g in &diff.added {
            writeln!(out, "[[group]]")?;
            writeln!(out, "map = \"{}\"", g.map)?;
            writeln!(out, "toggle = \"{}\"", g.toggle)?;
            if let Some(ref on) = g.on {
                writeln!(out, "on = \"{}\"", on)?;
            }
            if let Some(ref off) = g.off {
                writeln!(out, "off = \"{}\"", off)?;
            }
            if !g.candidates.is_empty() {
                let picked: HashSet<&str> = [
                    g.on.as_deref().unwrap_or(""),
                    g.off.as_deref().unwrap_or(""),
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
                let unpicked: Vec<_> = g
                    .candidates
                    .iter()
                    .filter(|(name, _, _)| !picked.contains(name.as_str()))
                    .collect();
                if !unpicked.is_empty() {
                    for (name, score, role) in &unpicked {
                        writeln!(out, "# {role} = \"{name}\" # {:.0}%", score * 100.0)?;
                    }
                }
            }
            writeln!(out)?;
        }
    }

    // Removed
    if !diff.removed.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "# ══ REMOVED ({count}) — in existing TOML but no longer in SC ══",
            count = diff.removed.len()
        )?;
        writeln!(
            out,
            "# These actions no longer exist in defaultProfile.xml."
        )?;
        writeln!(out, "# Consider removing them from toggle-groups.toml.")?;
        writeln!(out)?;

        for g in &diff.removed {
            writeln!(out, "# map = \"{}\"", g.map)?;
            writeln!(out, "# toggle = \"{}\"", g.toggle)?;
            if let Some(ref on) = g.on {
                writeln!(out, "# on = \"{}\"", on)?;
            }
            if let Some(ref off) = g.off {
                writeln!(out, "# off = \"{}\"", off)?;
            }
            writeln!(out)?;
        }
    }

    // Changed
    if !diff.changed.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "# ══ CHANGED ({count}) — new siblings detected ══",
            count = diff.changed.len()
        )?;
        writeln!(
            out,
            "# These groups exist in the TOML but the generator found new on/off siblings."
        )?;
        writeln!(out)?;

        for c in &diff.changed {
            writeln!(out, "# [{}.{}]", c.map, c.toggle)?;
            if let Some(ref old) = c.old_on {
                writeln!(out, "#   existing on  = \"{}\"", old)?;
            }
            if let Some(ref new) = c.new_on {
                if c.old_on.is_none() {
                    writeln!(out, "#   detected on  = \"{}\"  (currently not set)", new)?;
                }
            }
            if let Some(ref old) = c.old_off {
                writeln!(out, "#   existing off = \"{}\"", old)?;
            }
            if let Some(ref new) = c.new_off {
                if c.old_off.is_none() {
                    writeln!(out, "#   detected off = \"{}\"  (currently not set)", new)?;
                }
            }
            if !c.candidates.is_empty() {
                for (name, score, role) in &c.candidates {
                    writeln!(out, "#   {role} = \"{name}\" # {:.0}%", score * 100.0)?;
                }
            }
            writeln!(out)?;
        }
    }

    Ok(())
}

// ── Main ────────────────────────────────────────────────────────────────────────

fn resolve_install_path() -> Result<PathBuf> {
    let arg = std::env::args().nth(1);

    match arg {
        Some(ref path) => {
            let p = PathBuf::from(path);
            if p.is_file() && path.ends_with("Data.p4k") {
                Ok(p.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| p.clone()))
            } else if p.is_dir() {
                Ok(p)
            } else {
                bail!("Path is not a Data.p4k file or directory: {path}");
            }
        }
        None => {
            println!("Auto-discovering Star Citizen installations...");
            let installations = discovery::discover_installations();
            if installations.is_empty() {
                bail!(
                    "No Star Citizen installations found.\n\
                     Pass a path manually: toggle-groups-gen.exe \"C:\\path\\to\\LIVE\""
                );
            }

            let install = installations
                .iter()
                .find(|i| i.channel == Channel::Live)
                .unwrap_or(&installations[0]);

            println!(
                "Using {} installation at {}",
                install.channel,
                install.path.display()
            );
            Ok(install.path.clone())
        }
    }
}

fn run() -> Result<()> {
    let install_path = resolve_install_path()?;

    println!("Loading bindings from {}...", install_path.display());
    let bindings = streamdeck_starcitizen::bindings::load_bindings_defaults_only(&install_path)
        .context("Failed to load bindings")?;

    println!(
        "Parsed {} action maps, {} actions",
        bindings.map_count(),
        bindings.action_count()
    );

    let groups = discover_groups(&bindings);

    // The binary lives at .sdPlugin/bin/, the TOML at .sdPlugin/ (one level up)
    let plugin_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // bin/
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // .sdPlugin/
        .unwrap_or_else(|| PathBuf::from("."));

    let existing_path = plugin_dir.join("toggle-groups.toml");
    let existing = load_existing(&existing_path);

    if existing.is_empty() {
        // No existing file → full generation mode
        let out_path = plugin_dir.join("toggle-groups.toml");
        let mut file = std::fs::File::create(&out_path)
            .with_context(|| format!("Failed to create {}", out_path.display()))?;
        write_toml(&groups, &mut file)?;

        let with_siblings = groups
            .iter()
            .filter(|g| g.on.is_some() || g.off.is_some())
            .count();
        let toggle_only = groups.len() - with_siblings;

        println!();
        println!("Wrote {}", out_path.display());
        println!(
            "  {} groups total ({} with enable/disable, {} toggle-only)",
            groups.len(),
            with_siblings,
            toggle_only
        );
    } else {
        // Existing file found → diff mode
        println!(
            "Found existing {} ({} groups)",
            existing_path.display(),
            existing.len()
        );

        let diff = compute_diff(&existing, &groups);
        let out_path = plugin_dir.join("toggle-groups-diff.toml");

        let mut file = std::fs::File::create(&out_path)
            .with_context(|| format!("Failed to create {}", out_path.display()))?;
        write_diff(&diff, &mut file)?;

        println!();
        println!("Wrote {}", out_path.display());
        println!(
            "  {} new, {} removed, {} changed",
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len()
        );

        if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
            println!("  toggle-groups.toml is up to date!");
        }
    }

    Ok(())
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }

    // Pause so double-click users can read the output
    if std::env::args().len() <= 1 {
        println!();
        println!("Press Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}
