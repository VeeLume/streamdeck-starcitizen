pub mod autofill;
pub mod executor;
pub mod generator_config;
pub mod model;
pub mod overlay;
pub mod p4k;
pub mod parser;
pub mod translations;

use std::path::Path;

use anyhow::{Context, Result};
use tracing::{debug, info};

use self::model::ParsedBindings;
use self::overlay::UserOverride;

/// Action maps that should be hidden from the UI and skipped during autofill.
///
/// These maps are either not implemented in-game, debug-only, or not useful
/// for players.
pub const HIDDEN_ACTION_MAPS: &[&str] = &[
    "spaceship_auto_weapons", // AI auto-targeting controls, not player-facing
    "hacking",                // Not implemented in-game
    "debug",                  // Developer-only, non-functional for players
    "IFCS_controls",          // Cryptic A/B/X/Y options, not player-facing
    "flycam",                 // Internal camera tool
    "character_customizer",   // Character creator, not needed at runtime
    "RemoteRigidEntityController", // Internal entity controller
    "server_renderer",        // Server-side renderer controls
];

use self::overlay::{apply_overlay, parse_user_overlay};
use self::p4k::extract_profile_and_ini;
use self::parser::parse_default_profile;
use self::translations::parse_global_ini;

/// Result of loading bindings: merged data plus raw user overrides.
pub struct LoadedBindings {
    pub bindings: ParsedBindings,
    pub user_overrides: Vec<UserOverride>,
}

/// Load and parse all bindings from a Star Citizen installation.
///
/// Pipeline:
/// 1. Open `Data.p4k` once, extract both `defaultProfile.xml` and `global.ini`
/// 2. Parse translations from `global.ini`
/// 3. Parse the default profile XML with translations
/// 4. Apply user overlay from `actionmaps.xml` (if it exists)
///
/// Returns the merged bindings and the raw user overrides (needed to preserve
/// user customisations in the generated profile).
pub fn load_bindings(install_path: &Path) -> Result<LoadedBindings> {
    let p4k_path = install_path.join("Data.p4k");

    info!("Loading bindings from {}", p4k_path.display());

    // Step 1: Extract both files from a single archive open.
    let (profile_xml, ini_bytes) =
        extract_profile_and_ini(&p4k_path).context("Failed to extract from P4K")?;

    // Step 2: Extract translations
    let translations = parse_global_ini(&ini_bytes);
    debug!("Loaded {} translations", translations.len());

    // Step 3: Parse the profile
    let mut bindings =
        parse_default_profile(&profile_xml, &translations).context("Failed to parse profile")?;

    info!(
        "Parsed {} action maps, {} actions",
        bindings.map_count(),
        bindings.action_count()
    );

    // Step 4: Apply user overlay if present
    let user_overlay_path = install_path.join("user/client/0/Profiles/default/actionmaps.xml");
    let user_overrides = if user_overlay_path.exists() {
        debug!("Found user overlay at {}", user_overlay_path.display());
        let overlay_xml = std::fs::read_to_string(&user_overlay_path)
            .with_context(|| format!("Read {}", user_overlay_path.display()))?;
        let overrides = parse_user_overlay(&overlay_xml).context("Failed to parse user overlay")?;
        debug!("Applying {} user overrides", overrides.len());
        apply_overlay(&mut bindings, &overrides);
        overrides
    } else {
        debug!("No user overlay at {}", user_overlay_path.display());
        Vec::new()
    };

    Ok(LoadedBindings {
        bindings,
        user_overrides,
    })
}

/// Load default bindings only (no user overlay).
///
/// This runs steps 1-3 of [`load_bindings`] — extract from Data.p4k, load
/// translations, and parse the default profile — but skips the user overlay.
/// Used when generating bindings with "ignore user binds" enabled.
pub fn load_bindings_defaults_only(install_path: &Path) -> Result<ParsedBindings> {
    let p4k_path = install_path.join("Data.p4k");

    info!("Loading default-only bindings from {}", p4k_path.display());

    let (profile_xml, ini_bytes) =
        extract_profile_and_ini(&p4k_path).context("Failed to extract from P4K")?;

    let translations = parse_global_ini(&ini_bytes);
    debug!("Loaded {} translations", translations.len());

    let bindings =
        parse_default_profile(&profile_xml, &translations).context("Failed to parse profile")?;

    info!(
        "Parsed {} action maps, {} actions (defaults only)",
        bindings.map_count(),
        bindings.action_count()
    );

    Ok(bindings)
}
