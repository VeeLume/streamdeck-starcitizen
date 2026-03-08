use std::path::Path;

use anyhow::{Context, Result};
use svarog_cryxml::CryXml;
use svarog_p4k::P4kArchive;
use tracing::debug;

/// Extract `defaultProfile.xml` from Data.p4k and convert CryXmlB to text XML.
pub fn extract_default_profile(p4k_path: &Path) -> Result<String> {
    let archive =
        P4kArchive::open(p4k_path).with_context(|| format!("Open P4K: {}", p4k_path.display()))?;

    let entry = archive
        .find("Data/Libs/Config/defaultProfile.xml")
        .with_context(|| "defaultProfile.xml not found in P4K")?;

    let bytes = archive
        .read(&entry)
        .with_context(|| "Failed to read defaultProfile.xml")?;

    debug!(
        "Extracted defaultProfile.xml ({} bytes, cryxml={})",
        bytes.len(),
        CryXml::is_cryxml(&bytes)
    );

    convert_to_text_xml(&bytes, "defaultProfile.xml")
}

/// Extract `global.ini` from Data.p4k.
///
/// Returns raw bytes (UTF-16 LE encoded INI file).
pub fn extract_global_ini(p4k_path: &Path) -> Result<Vec<u8>> {
    let archive =
        P4kArchive::open(p4k_path).with_context(|| format!("Open P4K: {}", p4k_path.display()))?;

    let entry = archive
        .find("Data/Localization/english/global.ini")
        .with_context(|| "global.ini not found in P4K")?;

    let bytes = archive
        .read(&entry)
        .with_context(|| "Failed to read global.ini")?;

    debug!("Extracted global.ini ({} bytes)", bytes.len());
    Ok(bytes)
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
