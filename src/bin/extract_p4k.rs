use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use svarog_cryxml::CryXml;
use svarog_p4k::P4kArchive;

/// Files to extract from Data.p4k, with forward-slash paths matching P4K entries.
const EXTRACT_FILES: &[&str] = &[
    "Data/Libs/Config/defaultProfile.xml",
    "Data/Libs/Config/keybinding_localization.xml",
    "Data/Localization/english/global.ini",
];

/// Glob-style prefix directories — all entries under these are extracted.
const EXTRACT_DIRS: &[&str] = &["Data/Libs/Config/Mappings/"];

const P4K_PATH: &str = "C:/Games/StarCitizen/LIVE/Data.p4k";
const OUTPUT_DIR: &str = "p4k-extracted";

fn main() -> Result<()> {
    let p4k_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| P4K_PATH.to_string());

    let archive = P4kArchive::open(&p4k_path)
        .with_context(|| format!("Failed to open P4K archive: {p4k_path}"))?;

    let mut extracted = 0u32;
    let mut skipped = 0u32;

    for &path in EXTRACT_FILES {
        match extract_entry(&archive, path) {
            Ok(true) => extracted += 1,
            Ok(false) => skipped += 1,
            Err(e) => eprintln!("  ERROR: {path}: {e}"),
        }
    }

    // Walk all archive entries to find directory matches
    for entry in archive.entries() {
        let name = entry.name();
        let normalized = name.replace('\\', "/");
        for &dir in EXTRACT_DIRS {
            if normalized.starts_with(dir) {
                match extract_entry(&archive, &normalized) {
                    Ok(true) => extracted += 1,
                    Ok(false) => skipped += 1,
                    Err(e) => eprintln!("  ERROR: {normalized}: {e}"),
                }
            }
        }
    }

    println!("\nDone: {extracted} extracted, {skipped} unchanged");
    Ok(())
}

fn extract_entry(archive: &P4kArchive, path: &str) -> Result<bool> {
    let alt_path = path.replace('/', "\\");
    let entry = archive
        .find(path)
        .or_else(|| archive.find(&alt_path))
        .with_context(|| format!("Entry not found: {path}"))?;

    let out_path = Path::new(OUTPUT_DIR).join(path.replace('\\', "/"));

    let bytes = archive
        .read(&entry)
        .with_context(|| format!("Failed to read: {path}"))?;

    // Decode CryXmlB binary format to XML text
    let content = if CryXml::is_cryxml(&bytes) {
        let cryxml =
            CryXml::parse(&bytes).with_context(|| format!("Failed to parse CryXmlB: {path}"))?;
        cryxml
            .to_xml_string()
            .with_context(|| format!("Failed to convert CryXmlB: {path}"))?
            .into_bytes()
    } else {
        bytes
    };

    // Skip if file exists with same size (incremental)
    if out_path.exists() {
        let existing_len = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        if existing_len == content.len() as u64 {
            println!("  SKIP: {path} (unchanged)");
            return Ok(false);
        }
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    fs::write(&out_path, &content)
        .with_context(|| format!("Failed to write: {}", out_path.display()))?;

    println!("  OK: {path}");
    Ok(true)
}
