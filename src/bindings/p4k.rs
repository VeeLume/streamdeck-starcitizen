use std::path::Path;

use anyhow::{Context, Result};
use svarog_cryxml::CryXml;
use svarog_p4k::P4kArchive;
use tracing::debug;

/// Extract both `defaultProfile.xml` and `global.ini` from Data.p4k in a single
/// archive open, avoiding a second parse of the ~60K-entry central directory.
///
/// Returns `(profile_xml, ini_bytes)`.
pub fn extract_profile_and_ini(p4k_path: &Path) -> Result<(String, Vec<u8>)> {
    let archive =
        P4kArchive::open(p4k_path).with_context(|| format!("Open P4K: {}", p4k_path.display()))?;

    debug!("P4K opened ({} entries)", archive.entry_count());

    // Extract default profile
    let profile_entry = archive
        .find("Data/Libs/Config/defaultProfile.xml")
        .with_context(|| "defaultProfile.xml not found in P4K")?;

    let profile_bytes = archive
        .read(&profile_entry)
        .with_context(|| "Failed to read defaultProfile.xml")?;

    debug!(
        "Extracted defaultProfile.xml ({} bytes, cryxml={})",
        profile_bytes.len(),
        CryXml::is_cryxml(&profile_bytes)
    );

    let profile_xml = convert_to_text_xml(&profile_bytes, "defaultProfile.xml")?;

    // Extract global.ini
    let ini_entry = archive
        .find("Data/Localization/english/global.ini")
        .with_context(|| "global.ini not found in P4K")?;

    let ini_bytes = archive
        .read(&ini_entry)
        .with_context(|| "Failed to read global.ini")?;

    debug!("Extracted global.ini ({} bytes)", ini_bytes.len());

    // archive (mmap + entry table) dropped here
    Ok((profile_xml, ini_bytes))
}

/// Convert bytes to text XML, handling CryXmlB if needed.
fn convert_to_text_xml(bytes: &[u8], label: &str) -> Result<String> {
    if CryXml::is_cryxml(bytes) {
        let cryxml = CryXml::parse(bytes).with_context(|| format!("Parse CryXmlB: {label}"))?;
        cryxml
            .to_xml_string()
            .with_context(|| format!("Convert CryXmlB to XML: {label}"))
    } else {
        // Already text XML
        String::from_utf8(bytes.to_vec()).with_context(|| format!("Invalid UTF-8 in {label}"))
    }
}
