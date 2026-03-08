pub mod autofill;
pub mod executor;
pub mod model;
pub mod overlay;
pub mod p4k;
pub mod parser;
pub mod translations;

use std::path::Path;

use anyhow::{Context, Result};
use tracing::{debug, info};

use self::model::ParsedBindings;
use self::overlay::{apply_overlay, parse_user_overlay};
use self::p4k::{extract_default_profile, extract_global_ini};
use self::parser::parse_default_profile;
use self::translations::parse_global_ini;

/// Load and parse all bindings from a Star Citizen installation.
///
/// Pipeline:
/// 1. Extract `defaultProfile.xml` from `Data.p4k` (CryXmlB → text XML)
/// 2. Extract `global.ini` from `Data.p4k` (UTF-16 LE translations)
/// 3. Parse the default profile XML with translations
/// 4. Apply user overlay from `actionmaps.xml` (if it exists)
pub fn load_bindings(install_path: &Path) -> Result<ParsedBindings> {
    let p4k_path = install_path.join("Data.p4k");

    info!("Loading bindings from {}", p4k_path.display());

    // Step 1: Extract and convert default profile
    let profile_xml =
        extract_default_profile(&p4k_path).context("Failed to extract default profile")?;

    // Step 2: Extract translations
    let ini_bytes = extract_global_ini(&p4k_path).context("Failed to extract global.ini")?;
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
    if user_overlay_path.exists() {
        debug!("Found user overlay at {}", user_overlay_path.display());
        let overlay_xml = std::fs::read_to_string(&user_overlay_path)
            .with_context(|| format!("Read {}", user_overlay_path.display()))?;
        let overrides = parse_user_overlay(&overlay_xml).context("Failed to parse user overlay")?;
        debug!("Applying {} user overrides", overrides.len());
        apply_overlay(&mut bindings, &overrides);
    } else {
        debug!("No user overlay at {}", user_overlay_path.display());
    }

    Ok(bindings)
}
