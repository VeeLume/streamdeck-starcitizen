//! Developer tool: build a discovery report for toggle groups in Star Citizen.
//!
//! Not shipped with the plugin — run from the repo via `cargo run`. The report
//! is a developer aid for hand-curating `toggle-groups.toml`; it never writes
//! into the `.sdPlugin` directory.
//!
//! Identifies toggle actions using two XML signals:
//! - `<states>` child elements (explicit on/off, locked/unlocked, etc.)
//! - `activationMode="smart_toggle"`
//! - Name contains "toggle" (permissive fallback)
//!
//! Then finds enable/disable siblings via token overlap within the same actionmap.
//!
//! Usage:
//!   cargo run --bin report-builder                       # auto-discover LIVE
//!   cargo run --bin report-builder -- <p4k-or-dir>       # explicit install path
//!   cargo run --bin report-builder -- --out <toml-path>  # custom output path
//!
//! Default outputs are written to `<repo-root>/reports/`.

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use streamdeck_starcitizen::bindings::HIDDEN_ACTION_MAPS;
use streamdeck_starcitizen::bindings::model::{GameAction, ParsedBindings};
use streamdeck_starcitizen::discovery::{self, Channel};

// ── Toggle group model ──────────────────────────────────────────────────────────

struct ToggleGroup {
    map: String,
    /// Toggle action name; `None` for orphan on/off pairs without a toggle sibling.
    toggle: Option<String>,
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
        if is_disqualified_sibling(&other.name) {
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
    "on",
    "off",
    "enable",
    "disable",
    "open",
    "close",
    "lock",
    "unlock",
    "deploy",
    "retract",
    "engage",
    "disengage",
    "arm",
    "disarm",
    "activate",
    "deactivate",
    "raise",
    "lower",
    "mount",
    "dismount",
    "equip",
    "holster",
    "extend",
    "contract",
];

/// Suffixes that mark an action as an activation/directional variant rather
/// than an idempotent enable/disable. Names ending in these are never accepted
/// as on/off siblings of a toggle.
const DISQUALIFIED_SIBLING_SUFFIXES: &[&str] = &[
    "_hold",
    "_cycle",
    "_short",
    "_long",
    "_press",
    "_tap",
    "_up",
    "_down",
    "_increase",
    "_decrease",
    "_increment",
    "_decrement",
    "_max",
    "_min",
    "_abs",
    "_rel",
    "_button",
    "_wheel",
    "_analog",
];

fn is_disqualified_sibling(name: &str) -> bool {
    let lower = name.to_lowercase();
    DISQUALIFIED_SIBLING_SUFFIXES
        .iter()
        .any(|s| lower.ends_with(s))
}

fn classify_sibling(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    // Check for "on"-like suffixes
    if lower.ends_with("_on")
        || lower.contains("_enable")
        || lower.ends_with("_open")
        || lower.ends_with("_unlock")
        || lower.ends_with("_deploy")
        || lower.ends_with("_engage")
        || lower.ends_with("_arm")
        || lower.ends_with("_activate")
        || lower.ends_with("_raise")
        || lower.ends_with("_mount")
        || lower.ends_with("_equip")
        || lower.ends_with("_extend")
    {
        return Some("on");
    }
    // Check for "off"-like suffixes
    if lower.ends_with("_off")
        || lower.contains("_disable")
        || lower.ends_with("_close")
        || lower.ends_with("_lock")
        || lower.ends_with("_retract")
        || lower.ends_with("_disengage")
        || lower.ends_with("_disarm")
        || lower.ends_with("_deactivate")
        || lower.ends_with("_lower")
        || lower.ends_with("_dismount")
        || lower.ends_with("_holster")
        || lower.ends_with("_contract")
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

            // Only auto-pick siblings with strong overlap. Below this we keep
            // them as `# alt` candidates so the curator can see them, but we
            // don't claim them — they're more likely to be wrong than right.
            const AUTO_PICK_THRESHOLD: f64 = 0.5;

            for (sib_name, score, role) in &siblings {
                if *score < AUTO_PICK_THRESHOLD {
                    continue;
                }
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
                toggle: Some(action.name.to_string()),
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
                    && !is_disqualified_sibling(&a.name)
            })
            .collect();

        let off_actions: Vec<_> = map
            .actions
            .iter()
            .filter(|a| {
                !claimed.contains(a.name.as_ref())
                    && classify_sibling(&a.name) == Some("off")
                    && !is_redundant_variant(&a.name, &map_action_names)
                    && !is_disqualified_sibling(&a.name)
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

            // Emit any reasonable token-overlap match. The score is shown in
            // the report comment so the curator can judge weak matches.
            if best_score < 0.6 {
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

            groups.push(ToggleGroup {
                map: map.name.to_string(),
                toggle: if toggle_name.is_empty() {
                    None
                } else {
                    Some(toggle_name)
                },
                on: Some(on_a.name.to_string()),
                off: Some(off_a.name.to_string()),
                candidates: vec![],
            });
        }
    }

    groups
}

// ── TOML output ─────────────────────────────────────────────────────────────────

fn write_toml(
    groups: &[ToggleGroup],
    bindings: &ParsedBindings,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(out, "# Toggle groups discovery report.")?;
    writeln!(
        out,
        "# Generated by `cargo run --bin report-builder` from defaultProfile.xml."
    )?;
    writeln!(
        out,
        "# Use this report as input when hand-curating icu.veelume.starcitizen.sdPlugin/toggle-groups.toml."
    )?;
    writeln!(out)?;
    writeln!(out, "# Schema:")?;
    writeln!(
        out,
        "#   id           required, stable slug used as the saved setting"
    )?;
    writeln!(
        out,
        "#   name         required, label shown in the PI dropdown"
    )?;
    writeln!(
        out,
        "#   description  optional, tooltip shown under the dropdown"
    )?;
    writeln!(
        out,
        "#   map          required, action map containing the binding"
    )?;
    writeln!(out, "#   toggle       optional, toggle action name")?;
    writeln!(
        out,
        "#   on / off     optional, idempotent enable/disable actions"
    )?;
    writeln!(
        out,
        "#   label_on     optional, button label when state is ON"
    )?;
    writeln!(
        out,
        "#   label_off    optional, button label when state is OFF"
    )?;
    writeln!(out, "#   start_on     optional bool (default false)")?;
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

        let primary = g
            .toggle
            .as_deref()
            .or(g.on.as_deref())
            .or(g.off.as_deref())
            .unwrap_or("?");
        let id = format!("{}.{}", g.map, primary);
        let derived_name =
            derive_group_name(bindings, &g.map, g.toggle.as_deref(), g.on.as_deref());

        // Action context as comments — gives the curator everything they need
        // to confirm/rename without cross-referencing the XML.
        write_action_comment(out, "toggle", bindings, &g.map, g.toggle.as_deref())?;
        write_action_comment(out, "on    ", bindings, &g.map, g.on.as_deref())?;
        write_action_comment(out, "off   ", bindings, &g.map, g.off.as_deref())?;

        writeln!(out, "[[group]]")?;
        writeln!(out, "id = \"{id}\"")?;
        writeln!(out, "name = \"{}\"", escape_toml(&derived_name))?;
        writeln!(out, "map = \"{}\"", g.map)?;
        if let Some(ref t) = g.toggle {
            writeln!(out, "toggle = \"{t}\"")?;
        }
        if let Some(ref on) = g.on {
            writeln!(out, "on = \"{on}\"")?;
        }
        if let Some(ref off) = g.off {
            writeln!(out, "off = \"{off}\"")?;
        }
        // Show un-picked sibling candidates as comments to help curation
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
            for (name, score, role) in &unpicked {
                writeln!(out, "# alt {role} = \"{name}\" # {:.0}%", score * 100.0)?;
            }
        }
        writeln!(out)?;
    }

    Ok(())
}

/// Look up an action by name in a map and return it.
fn find_action<'a>(
    bindings: &'a ParsedBindings,
    map_name: &str,
    action_name: &str,
) -> Option<&'a GameAction> {
    bindings
        .action_maps
        .iter()
        .find(|m| m.name.as_ref() == map_name)?
        .actions
        .iter()
        .find(|a| a.name.as_ref() == action_name)
}

/// Write a `# <slot> "<name>" — <ui_label> [states: ...] [activation: ...]` line.
fn write_action_comment(
    out: &mut dyn Write,
    slot: &str,
    bindings: &ParsedBindings,
    map_name: &str,
    action_name: Option<&str>,
) -> std::io::Result<()> {
    let Some(name) = action_name else {
        return Ok(());
    };
    let Some(action) = find_action(bindings, map_name, name) else {
        writeln!(out, "# {slot} \"{name}\" — (action not found in map)")?;
        return Ok(());
    };

    let mut suffix = String::new();
    if !action.states.is_empty() {
        let states: Vec<&str> = action.states.iter().map(|s| s.name.as_str()).collect();
        suffix.push_str(&format!(" [states: {}]", states.join("/")));
    }
    if let Some(ref am) = action.activation_mode {
        suffix.push_str(&format!(" [activation: {am}]"));
    }

    writeln!(out, "# {slot} \"{name}\" — {}{}", action.ui_label, suffix)?;
    Ok(())
}

/// Derive a default `name` for the dropdown from the toggle (or on) action's ui_label.
fn derive_group_name(
    bindings: &ParsedBindings,
    map_name: &str,
    toggle: Option<&str>,
    on: Option<&str>,
) -> String {
    let action_name = toggle.or(on).unwrap_or("");
    let Some(action) = find_action(bindings, map_name, action_name) else {
        return action_name.replace('_', " ");
    };

    let label = action.ui_label.as_ref();
    // Strip common toggle/set verbs from the start so the name reads as a noun.
    for prefix in [
        "Toggle ",
        "Set ",
        "Enable ",
        "Disable ",
        "Activate ",
        "Deactivate ",
    ] {
        if let Some(rest) = label.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    label.to_string()
}

/// Escape `"` and `\` for inclusion in a TOML basic string.
fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ── Main ────────────────────────────────────────────────────────────────────────

struct Args {
    install_path: Option<String>,
    out_path: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut install_path = None;
    let mut out_path = None;
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out" | "-o" => {
                let v = iter.next().context("--out requires a path argument")?;
                out_path = Some(PathBuf::from(v));
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ if arg.starts_with("--") => bail!("Unknown flag: {arg}"),
            _ if install_path.is_none() => install_path = Some(arg),
            _ => bail!("Unexpected positional argument: {arg}"),
        }
    }
    Ok(Args {
        install_path,
        out_path,
    })
}

fn print_usage() {
    println!(
        "Usage: cargo run --bin report-builder [-- <install-or-p4k-path>] [--out <toml>]\n\
         \n\
         Without arguments, auto-discovers the LIVE installation and writes the\n\
         report to <repo-root>/reports/toggle-groups-report.toml.\n\
         \n\
         The report never overwrites the curated toggle-groups.toml inside the\n\
         .sdPlugin directory — hand-curate that file using the report as input."
    );
}

fn resolve_install_path(arg: Option<&str>) -> Result<PathBuf> {
    match arg {
        Some(path) => {
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
                     Pass a path manually: cargo run --bin report-builder -- \"C:\\path\\to\\LIVE\""
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

/// Walk up from CWD to find the repo root (directory containing `Cargo.toml`).
fn find_repo_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut probe = cwd.as_path();
    loop {
        if probe.join("Cargo.toml").is_file() {
            return probe.to_path_buf();
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => break,
        }
    }
    cwd
}

/// Default report output: `<repo-root>/reports/toggle-groups-report.toml`.
fn default_report_path() -> PathBuf {
    find_repo_root()
        .join("reports")
        .join("toggle-groups-report.toml")
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let install_path = resolve_install_path(args.install_path.as_deref())?;

    println!("Loading bindings from {}...", install_path.display());
    let bindings = streamdeck_starcitizen::bindings::load_bindings_defaults_only(&install_path)
        .context("Failed to load bindings")?;

    println!(
        "Parsed {} action maps, {} actions",
        bindings.map_count(),
        bindings.action_count()
    );

    let groups = discover_groups(&bindings);

    let report_path = args.out_path.clone().unwrap_or_else(default_report_path);
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut file = std::fs::File::create(&report_path)
        .with_context(|| format!("Failed to create {}", report_path.display()))?;
    write_toml(&groups, &bindings, &mut file)?;

    let with_siblings = groups
        .iter()
        .filter(|g| g.on.is_some() || g.off.is_some())
        .count();
    let toggle_only = groups.len() - with_siblings;

    println!();
    println!("Wrote {}", report_path.display());
    println!(
        "  {} groups total ({} with enable/disable, {} toggle-only)",
        groups.len(),
        with_siblings,
        toggle_only
    );

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
