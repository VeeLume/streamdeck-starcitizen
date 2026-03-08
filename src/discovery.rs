use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{debug, warn};

// ── Channel ──────────────────────────────────────────────────────────────────

/// Star Citizen release channels, ordered by priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Live,
    Hotfix,
    Ptu,
    Eptu,
    TechPreview,
}

impl Channel {
    /// Lower is higher priority: LIVE > Hotfix > PTU > EPTU > TechPreview.
    pub fn priority(self) -> u8 {
        match self {
            Self::Live => 0,
            Self::Hotfix => 1,
            Self::Ptu => 2,
            Self::Eptu => 3,
            Self::TechPreview => 4,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Live => "LIVE",
            Self::Hotfix => "HOTFIX",
            Self::Ptu => "PTU",
            Self::Eptu => "EPTU",
            Self::TechPreview => "TECH",
        }
    }

    /// Parse a channel name from a launcher log line or directory name.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let upper = s.to_uppercase();
        match upper.as_str() {
            "LIVE" => Some(Self::Live),
            "HOTFIX" => Some(Self::Hotfix),
            "PTU" => Some(Self::Ptu),
            "EPTU" => Some(Self::Eptu),
            "TECHPREVIEW" | "TECH-PREVIEW" | "TECH" => Some(Self::TechPreview),
            _ => None,
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

// ── Installation ─────────────────────────────────────────────────────────────

/// A discovered Star Citizen installation.
#[derive(Debug, Clone)]
pub struct Installation {
    pub channel: Channel,
    pub path: PathBuf,
    pub version: String,
    #[allow(dead_code)] // Parsed from build_manifest.id, may be shown in PI later
    pub branch: String,
    #[allow(dead_code)] // Parsed from build_manifest.id, may be shown in PI later
    pub build_id: String,
}

impl Installation {
    /// Short version string for display (e.g. "4.6" from "4.6.1.0").
    pub fn short_version(&self) -> &str {
        // Take up to the second dot: "4.6.1.0" -> "4.6"
        let mut dots = 0;
        for (i, c) in self.version.char_indices() {
            if c == '.' {
                dots += 1;
                if dots == 2 {
                    return &self.version[..i];
                }
            }
        }
        &self.version
    }
}

// ── Build manifest ───────────────────────────────────────────────────────────

/// Fields from `build_manifest.id` JSON.
///
/// Supports two formats:
/// - **New (v2):** `{ "Data": { "Branch": "...", "Version": "4.6.173.39432", ... } }`
/// - **Legacy:** `{ "RequestedP4kFileName": "Data_4.6.1.0.p4k", "Branch": "...", "BuildId": "..." }`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BuildManifest {
    /// New format: fields nested under "Data"
    #[serde(default)]
    data: Option<BuildManifestData>,
    /// Legacy format: P4K filename at root level
    #[serde(default)]
    requested_p4k_file_name: Option<String>,
    /// Legacy format: branch at root level
    #[serde(default)]
    branch: Option<String>,
    /// Legacy format: build ID at root level
    #[serde(default)]
    build_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BuildManifestData {
    #[serde(default)]
    version: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    build_id: String,
}

/// Read and parse the `build_manifest.id` JSON file in an installation directory.
pub fn read_build_manifest(install_path: &Path) -> Result<(String, String, String)> {
    let manifest_path = install_path.join("build_manifest.id");
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: BuildManifest = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    // New format: extract from nested Data object
    if let Some(data) = &manifest.data
        && !data.version.is_empty()
    {
        return Ok((
            data.version.clone(),
            data.branch.clone(),
            data.build_id.clone(),
        ));
    }

    // Legacy format: extract version from P4K filename
    let version = manifest
        .requested_p4k_file_name
        .as_deref()
        .and_then(|s| s.strip_prefix("Data_"))
        .and_then(|s| s.strip_suffix(".p4k"))
        .unwrap_or("")
        .to_string();

    Ok((
        version,
        manifest.branch.unwrap_or_default(),
        manifest.build_id.unwrap_or_default(),
    ))
}

// ── Launcher log parsing ─────────────────────────────────────────────────────

/// Default path to the RSI Launcher log file.
pub fn launcher_log_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(appdata).join("rsilauncher/logs/log.log")
}

/// Parse all launch entries from the RSI Launcher log.
///
/// Supports both log formats:
/// - **v2.x (JSON):** `{ "t":"...", "[main][info] ": "Launching Star Citizen LIVE from (C:\\Games\\StarCitizen\\LIVE)" }`
/// - **Legacy:** `[Launcher::launch] Launching Star Citizen LIVE from (D:\StarCitizen\LIVE)`
pub fn parse_launcher_log(log_path: &Path) -> Vec<(Channel, PathBuf)> {
    let content = match std::fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read launcher log at {}: {e}", log_path.display());
            return Vec::new();
        }
    };

    let mut results = Vec::new();
    for line in content.lines() {
        if let Some(entry) = extract_launch_entry(line) {
            results.push(entry);
        }
    }
    results
}

/// Extract a launch entry from a single log line, supporting both old and new formats.
fn extract_launch_entry(line: &str) -> Option<(Channel, PathBuf)> {
    const MARKER: &str = "Launching Star Citizen ";

    // Find the marker anywhere in the line (works for both JSON and legacy formats)
    let marker_pos = line.find(MARKER)?;
    let rest = &line[marker_pos + MARKER.len()..];

    // rest = "LIVE from (C:\\Games\\StarCitizen\\LIVE)..." (may have trailing JSON chars)
    let from_idx = rest.find(" from (")?;
    let channel_str = &rest[..from_idx];
    let path_start = from_idx + " from (".len();
    let path_end = rest[path_start..].find(')')?;
    let path_str = &rest[path_start..path_start + path_end];

    // In JSON log format, backslashes are escaped: C:\\Games → C:\Games
    let path_str = path_str.replace("\\\\", "\\");

    let channel = Channel::from_str_loose(channel_str)?;
    Some((channel, PathBuf::from(path_str)))
}

/// Detect which channel was launched from the process/application name.
///
/// The Stream Deck SDK provides the application name (e.g. "StarCitizen.exe").
/// If the path or name contains PTU/EPTU/HOTFIX/TECH, we can infer the channel.
pub fn detect_channel_from_app(app_name: &str) -> Option<Channel> {
    let upper = app_name.to_uppercase();
    if upper.contains("EPTU") {
        Some(Channel::Eptu)
    } else if upper.contains("PTU") {
        Some(Channel::Ptu)
    } else if upper.contains("HOTFIX") {
        Some(Channel::Hotfix)
    } else if upper.contains("TECHPREVIEW") || upper.contains("TECH-PREVIEW") {
        Some(Channel::TechPreview)
    } else {
        // Default to LIVE — StarCitizen.exe without qualifiers is the LIVE build
        Some(Channel::Live)
    }
}

// ── Discovery orchestrator ───────────────────────────────────────────────────

/// Discover all Star Citizen installations by parsing the RSI Launcher log.
///
/// Returns installations sorted by channel priority (LIVE first).
pub fn discover_installations() -> Vec<Installation> {
    let log_path = launcher_log_path();
    discover_installations_from(&log_path)
}

/// Discover installations from a specific log path (useful for testing).
pub fn discover_installations_from(log_path: &Path) -> Vec<Installation> {
    let entries = parse_launcher_log(log_path);

    // Deduplicate by channel — keep the last-seen path for each channel
    let mut by_channel = std::collections::HashMap::new();
    for (channel, path) in entries {
        by_channel.insert(channel, path);
    }

    let mut installations = Vec::new();
    for (channel, path) in by_channel {
        // Only include installations where the directory still exists
        if !path.exists() {
            debug!(
                "Skipping {channel} at {} — directory not found",
                path.display()
            );
            continue;
        }

        match read_build_manifest(&path) {
            Ok((version, branch, build_id)) => {
                installations.push(Installation {
                    channel,
                    path,
                    version,
                    branch,
                    build_id,
                });
            }
            Err(e) => {
                warn!("Skipping {channel} at {} — {e}", path.display());
            }
        }
    }

    // Sort by channel priority
    installations.sort_by_key(|i| i.channel.priority());
    installations
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_channel_names() {
        assert_eq!(Channel::from_str_loose("LIVE"), Some(Channel::Live));
        assert_eq!(Channel::from_str_loose("PTU"), Some(Channel::Ptu));
        assert_eq!(Channel::from_str_loose("EPTU"), Some(Channel::Eptu));
        assert_eq!(Channel::from_str_loose("Hotfix"), Some(Channel::Hotfix));
        assert_eq!(
            Channel::from_str_loose("TechPreview"),
            Some(Channel::TechPreview)
        );
        assert_eq!(Channel::from_str_loose("unknown"), None);
    }

    #[test]
    fn channel_priority_order() {
        assert!(Channel::Live.priority() < Channel::Hotfix.priority());
        assert!(Channel::Hotfix.priority() < Channel::Ptu.priority());
        assert!(Channel::Ptu.priority() < Channel::Eptu.priority());
        assert!(Channel::Eptu.priority() < Channel::TechPreview.priority());
    }

    #[test]
    fn parse_launcher_log_legacy_format() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            "2024-01-01 [Launcher::launch] Launching Star Citizen LIVE from (D:\\StarCitizen\\LIVE)"
        )
        .unwrap();
        writeln!(
            tmp,
            "2024-01-02 [Launcher::launch] Launching Star Citizen PTU from (D:\\StarCitizen\\PTU)"
        )
        .unwrap();
        writeln!(tmp, "some other log line").unwrap();
        writeln!(
            tmp,
            "2024-01-03 [Launcher::launch] Launching Star Citizen LIVE from (E:\\SC\\LIVE)"
        )
        .unwrap();

        let results = parse_launcher_log(tmp.path());
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, Channel::Live);
        assert_eq!(results[0].1, PathBuf::from("D:\\StarCitizen\\LIVE"));
        assert_eq!(results[1].0, Channel::Ptu);
        assert_eq!(results[2].0, Channel::Live);
        assert_eq!(results[2].1, PathBuf::from("E:\\SC\\LIVE"));
    }

    #[test]
    fn parse_launcher_log_json_format() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"{{ "t":"2025-05-03 17:38:04.649", "[main][info] ": "Launching Star Citizen LIVE from (C:\\Games\\StarCitizen\\LIVE)"  }},"#
        )
        .unwrap();
        writeln!(
            tmp,
            r#"{{ "t":"2025-05-03 18:00:42.832", "[main][info] ": "Launching Star Citizen PTU from (C:\\Games\\StarCitizen\\PTU)"  }},"#
        )
        .unwrap();
        writeln!(
            tmp,
            r#"{{ "t":"...", "[main][info] ": "Checking for update" }},"#
        )
        .unwrap();

        let results = parse_launcher_log(tmp.path());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, Channel::Live);
        assert_eq!(results[0].1, PathBuf::from("C:\\Games\\StarCitizen\\LIVE"));
        assert_eq!(results[1].0, Channel::Ptu);
        assert_eq!(results[1].1, PathBuf::from("C:\\Games\\StarCitizen\\PTU"));
    }

    #[test]
    fn parse_launcher_log_returns_all_entries() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"{{ "t":"...", "[main][info] ": "Launching Star Citizen LIVE from (C:\\SC\\LIVE)" }},"#
        )
        .unwrap();
        writeln!(
            tmp,
            r#"{{ "t":"...", "[main][info] ": "Launching Star Citizen PTU from (C:\\SC\\PTU)" }},"#
        )
        .unwrap();

        let entries = parse_launcher_log(tmp.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.last().unwrap().0, Channel::Ptu);
    }

    #[test]
    fn short_version_parsing() {
        let inst = Installation {
            channel: Channel::Live,
            path: PathBuf::from("C:\\SC\\LIVE"),
            version: "4.6.1.0".to_string(),
            branch: "sc-alpha-4.6.1".to_string(),
            build_id: "12345".to_string(),
        };
        assert_eq!(inst.short_version(), "4.6");

        let inst2 = Installation {
            version: "4.6".to_string(),
            ..inst.clone()
        };
        assert_eq!(inst2.short_version(), "4.6");
    }

    #[test]
    fn detect_channel_from_app_name() {
        assert_eq!(
            detect_channel_from_app("StarCitizen.exe"),
            Some(Channel::Live)
        );
        assert_eq!(
            detect_channel_from_app("StarCitizen_PTU.exe"),
            Some(Channel::Ptu)
        );
        assert_eq!(
            detect_channel_from_app("StarCitizen_EPTU.exe"),
            Some(Channel::Eptu)
        );
    }

    #[test]
    fn discover_from_real_launcher_log() {
        let log_path = launcher_log_path();
        if !log_path.exists() {
            eprintln!("Skipping: launcher log not found at {}", log_path.display());
            return;
        }

        let entries = parse_launcher_log(&log_path);
        assert!(
            !entries.is_empty(),
            "No launch entries found in {}",
            log_path.display()
        );

        eprintln!("Found {} launch entries in launcher log", entries.len());

        // Check last entry has a valid path
        let (channel, path) = entries.last().unwrap();
        eprintln!("Last launch: {} from {}", channel, path.display());

        // Try full discovery (reads build manifests, filters missing dirs)
        let installations = discover_installations_from(&log_path);
        eprintln!("Discovered {} installations:", installations.len());
        for inst in &installations {
            eprintln!(
                "  {} v{} at {}",
                inst.channel,
                inst.version,
                inst.path.display()
            );
        }

        assert!(
            !installations.is_empty(),
            "Expected at least one installation"
        );

        // LIVE should be first (highest priority)
        assert_eq!(
            installations[0].channel,
            Channel::Live,
            "First installation should be LIVE"
        );
    }

    #[test]
    fn parse_legacy_manifest() {
        let json = r#"{"RequestedP4kFileName":"Data_4.6.1.0.p4k","Branch":"sc-alpha-4.6.1","BuildId":"12345"}"#;
        let manifest: super::BuildManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.data.is_none());
        assert_eq!(
            manifest
                .requested_p4k_file_name
                .as_deref()
                .and_then(|s| s.strip_prefix("Data_"))
                .and_then(|s| s.strip_suffix(".p4k")),
            Some("4.6.1.0")
        );
    }

    #[test]
    fn parse_new_manifest() {
        let json =
            r#"{"Data":{"Branch":"sc-alpha-4.6.0","BuildId":"None","Version":"4.6.173.39432"}}"#;
        let manifest: super::BuildManifest = serde_json::from_str(json).unwrap();
        let data = manifest.data.unwrap();
        assert_eq!(data.version, "4.6.173.39432");
        assert_eq!(data.branch, "sc-alpha-4.6.0");
    }
}
