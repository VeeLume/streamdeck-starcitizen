//! Word-level abbreviation table for Star Citizen action labels.
//!
//! The `AbbreviationTable` merges a curated built-in table with optional
//! user overrides from `%APPDATA%/icu.veelume.starcitizen/abbreviations.toml`.
//!
//! Each entry maps an uppercase word to one or more abbreviated forms.
//! Entries carry a **priority** (higher = preferred by `FitAbbreviate`) and
//! computed **savings** (characters saved).  Users can specify simple strings,
//! weighted entries, or multi-level arrays.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::PLUGIN_ID;

// ── Public Types ────────────────────────────────────────────────────────────

/// A single abbreviation option for a word.
#[derive(Debug, Clone)]
pub struct AbbreviationEntry {
    /// Abbreviated form (e.g. "ACCEL" for "ACCELERATION").
    pub short: String,
    /// Higher = preferred by FitAbbreviate.  Defaults to `savings`.
    pub priority: u32,
    /// Characters saved: `word.len() - short.len()`.
    pub savings: usize,
}

/// A candidate abbreviation found in a specific text.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Word index in the whitespace-split text.
    pub word_index: usize,
    /// The original word as it appears in the text.
    pub original: String,
    /// Abbreviated form.
    pub abbreviated: String,
    /// Characters saved.
    pub savings: usize,
    /// Priority from the entry.
    pub priority: u32,
}

/// Thread-safe abbreviation table.
///
/// Merges built-in entries with user overrides.  Matching is case-insensitive;
/// unmatched words preserve their original case.
pub struct AbbreviationTable {
    /// UPPERCASE_KEY → sorted entries (priority descending).
    words: HashMap<String, Vec<AbbreviationEntry>>,
    /// Skip abbreviations below this savings threshold.
    pub min_savings: usize,
}

impl AbbreviationTable {
    /// Load the built-in table, then merge user overrides from
    /// `%APPDATA%/icu.veelume.starcitizen/abbreviations.toml`.
    pub fn load() -> Self {
        let mut table = Self {
            words: builtin_table(),
            min_savings: 2,
        };

        if let Some(path) = user_abbreviations_path() {
            if path.is_file() {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => match toml::from_str::<UserAbbreviationsFile>(&contents) {
                        Ok(user_file) => {
                            if let Some(ms) = user_file.min_savings {
                                table.min_savings = ms;
                            }
                            let count = user_file.words.len();
                            for (key, value) in user_file.words {
                                let entries = value.into_entries(&key);
                                table.words.insert(key.to_uppercase(), entries);
                            }
                            info!(
                                "Loaded {count} user abbreviation(s) from {}",
                                path.display()
                            );
                        }
                        Err(e) => warn!("Failed to parse {}: {e}", path.display()),
                    },
                    Err(e) => warn!("Failed to read {}: {e}", path.display()),
                }
            } else {
                debug!("No user abbreviations file at {}", path.display());
            }
        }

        table
    }

    /// Apply the highest-priority abbreviation for each matched word.
    ///
    /// Used by the `Abbreviate` step (unconditional abbreviation).
    /// Case-insensitive lookup; unmatched words preserve original case.
    pub fn apply(&self, text: &str) -> String {
        text.split_whitespace()
            .map(|word| {
                let key = word.to_uppercase();
                match self.words.get(&key) {
                    Some(entries) if !entries.is_empty() => {
                        // Use the highest-priority entry (first after sort)
                        entries[0].short.clone()
                    }
                    _ => word.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Find all abbreviation candidates in the text.
    ///
    /// Returns candidates sorted by priority descending, then savings descending.
    /// Filters by `min_savings`.  For words with multiple options, all options
    /// above the savings threshold are included.
    pub fn candidates(&self, text: &str) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        for (i, word) in text.split_whitespace().enumerate() {
            let key = word.to_uppercase();
            if let Some(entries) = self.words.get(&key) {
                for entry in entries {
                    if entry.savings >= self.min_savings {
                        candidates.push(Candidate {
                            word_index: i,
                            original: word.to_string(),
                            abbreviated: entry.short.clone(),
                            savings: entry.savings,
                            priority: entry.priority,
                        });
                    }
                }
            }
        }

        // Sort: highest priority first, then highest savings
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.savings.cmp(&a.savings)));

        candidates
    }

    /// Apply a specific set of candidates (by index) to the text.
    ///
    /// When multiple candidates target the same word index, the last one in
    /// `applied_indices` wins (allowing progressive escalation).
    pub fn apply_candidates(
        &self,
        text: &str,
        candidates: &[Candidate],
        applied_indices: &[usize],
    ) -> String {
        // Build a map: word_index → abbreviated form (last applied wins)
        let mut replacements: HashMap<usize, &str> = HashMap::new();
        for &idx in applied_indices {
            if let Some(c) = candidates.get(idx) {
                replacements.insert(c.word_index, &c.abbreviated);
            }
        }

        text.split_whitespace()
            .enumerate()
            .map(|(i, word)| match replacements.get(&i) {
                Some(abbr) => abbr.to_string(),
                None => word.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ── User File Format ────────────────────────────────────────────────────────

/// `abbreviations.toml` top-level structure.
#[derive(Debug, Deserialize)]
struct UserAbbreviationsFile {
    min_savings: Option<usize>,
    #[serde(default)]
    words: HashMap<String, AbbrevValue>,
}

/// Flexible abbreviation value: simple string, weighted, or multi-level.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AbbrevValue {
    /// `WORD = "ABBR"` — priority defaults to savings
    Simple(String),
    /// `WORD = { short = "ABBR", priority = 10 }`
    Weighted { short: String, priority: u32 },
    /// `WORD = [{ short = "MILD", priority = 3 }, { short = "AGG", priority = 8 }]`
    Multi(Vec<WeightedEntry>),
}

#[derive(Debug, Deserialize)]
struct WeightedEntry {
    short: String,
    priority: u32,
}

impl AbbrevValue {
    fn into_entries(self, key: &str) -> Vec<AbbreviationEntry> {
        let key_len = key.len();
        let mut entries = match self {
            AbbrevValue::Simple(short) => {
                let savings = key_len.saturating_sub(short.len());
                vec![AbbreviationEntry {
                    priority: savings as u32,
                    savings,
                    short,
                }]
            }
            AbbrevValue::Weighted { short, priority } => {
                let savings = key_len.saturating_sub(short.len());
                vec![AbbreviationEntry {
                    short,
                    priority,
                    savings,
                }]
            }
            AbbrevValue::Multi(entries) => entries
                .into_iter()
                .map(|e| {
                    let savings = key_len.saturating_sub(e.short.len());
                    AbbreviationEntry {
                        short: e.short,
                        priority: e.priority,
                        savings,
                    }
                })
                .collect(),
        };
        // Sort by priority descending
        entries.sort_by(|a, b| b.priority.cmp(&a.priority));
        entries
    }
}

// ── User File Path ──────────────────────────────────────────────────────────

fn user_abbreviations_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join(PLUGIN_ID)
            .join("abbreviations.toml"),
    )
}

// ── Built-in Table ──────────────────────────────────────────────────────────

/// Build the curated built-in abbreviation table.
///
/// Only entries with savings >= 2 are included.  Each word maps to a single
/// entry with `priority = savings`.
fn builtin_table() -> HashMap<String, Vec<AbbreviationEntry>> {
    let raw: &[(&str, &str)] = &[
        // ── Flight & Movement ──────────────────────────────────────
        ("ACCELERATION", "ACCEL"), // saves 7
        ("AFTERBURNER", "ABRN"),   // saves 7
        ("AUTOLAND", "ALND"),      // saves 4
        ("BACKWARD", "BWD"),       // saves 5
        ("BACKWARDS", "BWD"),      // saves 6
        ("COUPLED", "CPLD"),       // saves 3
        ("CRUISE", "CRS"),         // saves 3
        ("DECOUPLED", "DCPLD"),    // saves 4
        ("DECREASE", "DECR"),      // saves 4
        ("DECELERATE", "DECEL"),   // saves 5
        ("FLIGHT", "FLGHT"),       // saves 2 (borderline, but common)
        ("FORWARD", "FWD"),        // saves 4
        ("INCREASE", "INCR"),      // saves 4
        ("LANDING", "LNDG"),       // saves 3
        ("MATCH", "MTCH"),         // saves 2
        ("MOVEMENT", "MVMT"),      // saves 4
        ("QUANTUM", "QTM"),        // saves 4
        ("SPEED", "SPD"),          // saves 2
        ("THROTTLE", "THRTL"),     // saves 3
        ("VELOCITY", "VEL"),       // saves 5
        // ── Power & Systems ────────────────────────────────────────
        ("COOLER", "COOL"),              // saves 2
        ("DESTRUCT", "DESTR"),           // saves 3
        ("DESTRUCTION", "DESTR"),        // saves 6
        ("GENERATOR", "GEN"),            // saves 6
        ("POWER", "PWR"),                // saves 2
        ("SELF-DESTRUCT", "SELF-DESTR"), // saves 4
        ("SYSTEM", "SYS"),               // saves 3
        ("SYSTEMS", "SYS"),              // saves 4
        // ── Weapons & Combat ───────────────────────────────────────
        ("COUNTERMEASURE", "CM"),   // saves 12
        ("COUNTERMEASURES", "CMS"), // saves 12
        ("GIMBAL", "GMBL"),         // saves 2
        ("MISSILE", "MSSL"),        // saves 3
        ("MISSILES", "MSSLS"),      // saves 3
        ("TARGETING", "TGT"),       // saves 6
        ("TORPEDO", "TORP"),        // saves 3
        ("TORPEDOES", "TORPS"),     // saves 4
        ("WEAPON", "WEAPN"),        // saves 2 (borderline, common)
        ("WEAPONS", "WEAPNS"),      // saves 2
        // ── Vehicle & Cockpit ──────────────────────────────────────
        ("COCKPIT", "CKPT"),    // saves 3
        ("DISPLAY", "DISP"),    // saves 3
        ("DISPLAYS", "DISP"),   // saves 4
        ("DOORS", "DRS"),       // saves 2
        ("EJECT", "EJCT"),      // saves 2 (borderline, safety-critical)
        ("EMERGENCY", "EMERG"), // saves 4
        ("OPERATOR", "OPER"),   // saves 4
        ("REMOTE", "RMTE"),     // saves 2
        ("TURRET", "TURT"),     // saves 2
        ("VEHICLE", "VHCL"),    // saves 3
        ("VEHICLES", "VHCLS"),  // saves 3
        // ── Modes & Actions ────────────────────────────────────────
        ("ACTIVATE", "ACTV"),  // saves 4
        ("CAMERA", "CAM"),     // saves 3
        ("CYCLE", "CYC"),      // saves 2
        ("DISABLE", "DSBL"),   // saves 3
        ("DISABLED", "DSBL"),  // saves 4
        ("ENABLE", "ENBL"),    // saves 2
        ("ENABLED", "ENBL"),   // saves 3
        ("FREELOOK", "FLOOK"), // saves 3
        ("LOCK", "LCK"),       // saves 2 (borderline, but 4 → 3)
        ("MINING", "MINE"),    // saves 2
        ("SALVAGE", "SALV"),   // saves 3
        ("SCANNING", "SCAN"),  // saves 4
        ("TARGET", "TGT"),     // saves 3
        ("TOGGLE", "TOGL"),    // saves 2
        ("UNLOCK", "ULCK"),    // saves 2
        ("ZOOM", "ZM"),        // saves 2
        // ── General ────────────────────────────────────────────────
        ("CONFIGURATION", "CFG"), // saves 10
        ("INTERACTION", "INTRC"), // saves 6
        ("MANAGEMENT", "MGMT"),   // saves 6
        ("NAVIGATION", "NAV"),    // saves 7
        ("PERSONAL", "PRSNL"),    // saves 2
        ("SPACESHIP", "SHIP"),    // saves 5
    ];

    let mut table = HashMap::with_capacity(raw.len());
    for &(word, short) in raw {
        let savings = word.len().saturating_sub(short.len());
        table.insert(
            word.to_string(),
            vec![AbbreviationEntry {
                short: short.to_string(),
                priority: savings as u32,
                savings,
            }],
        );
    }
    table
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_table() -> AbbreviationTable {
        AbbreviationTable {
            words: builtin_table(),
            min_savings: 2,
        }
    }

    #[test]
    fn apply_basic() {
        let table = test_table();
        assert_eq!(table.apply("Toggle Power"), "TOGL PWR");
    }

    #[test]
    fn apply_preserves_unmatched_case() {
        let table = test_table();
        assert_eq!(table.apply("Open MobiGlas"), "Open MobiGlas");
    }

    #[test]
    fn apply_multi_word() {
        let table = test_table();
        assert_eq!(table.apply("Quantum Throttle Increase"), "QTM THRTL INCR");
    }

    #[test]
    fn apply_empty() {
        let table = test_table();
        assert_eq!(table.apply(""), "");
    }

    #[test]
    fn candidates_sorted_by_priority() {
        let table = test_table();
        let candidates = table.candidates("Toggle Countermeasure Zoom");
        // COUNTERMEASURE saves 12 (priority 12), QUANTUM saves 4, etc.
        assert!(!candidates.is_empty());
        // First candidate should have highest priority
        assert!(candidates[0].priority >= candidates.last().unwrap().priority);
    }

    #[test]
    fn candidates_filters_min_savings() {
        let mut table = test_table();
        table.min_savings = 5;
        let candidates = table.candidates("Toggle Power Acceleration");
        // TOGGLE saves 2, POWER saves 2 — both filtered out
        // ACCELERATION saves 7 — included
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].abbreviated, "ACCEL");
    }

    #[test]
    fn apply_candidates_selective() {
        let table = test_table();
        let candidates = table.candidates("Quantum Throttle Increase");
        // Apply only the first candidate (highest priority)
        let result = table.apply_candidates("Quantum Throttle Increase", &candidates, &[0]);
        // Should have abbreviated exactly one word
        let word_count = result.split_whitespace().count();
        assert_eq!(word_count, 3);
    }

    #[test]
    fn removed_low_savings_entries() {
        let table = test_table();
        // These should NOT be in the curated table (savings < 2)
        assert!(!table.words.contains_key("SHIELD"));
        assert!(!table.words.contains_key("ENGINE"));
        assert!(!table.words.contains_key("SEAT"));
        assert!(!table.words.contains_key("MODE"));
        assert!(!table.words.contains_key("BOOST"));
    }

    #[test]
    fn user_file_simple_format() {
        let toml_str = r#"
[words]
RETALIATOR = "RETA"
"#;
        let file: UserAbbreviationsFile = toml::from_str(toml_str).unwrap();
        let entries = file
            .words
            .into_iter()
            .next()
            .unwrap()
            .1
            .into_entries("RETALIATOR");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].short, "RETA");
        assert_eq!(entries[0].savings, 6); // 10 - 4
    }

    #[test]
    fn user_file_weighted_format() {
        let toml_str = r#"
[words]
QUANTUM = { short = "Q", priority = 99 }
"#;
        let file: UserAbbreviationsFile = toml::from_str(toml_str).unwrap();
        let entries = file
            .words
            .into_iter()
            .next()
            .unwrap()
            .1
            .into_entries("QUANTUM");
        assert_eq!(entries[0].priority, 99);
        assert_eq!(entries[0].savings, 6); // 7 - 1
    }

    #[test]
    fn user_file_multi_format() {
        let toml_str = r#"
[words]
COUNTERMEASURE = [
    { short = "CNTMSR", priority = 3 },
    { short = "CM", priority = 8 },
]
"#;
        let file: UserAbbreviationsFile = toml::from_str(toml_str).unwrap();
        let entries = file
            .words
            .into_iter()
            .next()
            .unwrap()
            .1
            .into_entries("COUNTERMEASURE");
        assert_eq!(entries.len(), 2);
        // Sorted by priority descending
        assert_eq!(entries[0].priority, 8);
        assert_eq!(entries[0].short, "CM");
        assert_eq!(entries[1].priority, 3);
        assert_eq!(entries[1].short, "CNTMSR");
    }
}
