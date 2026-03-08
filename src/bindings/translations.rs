use std::collections::HashMap;
use std::sync::Arc;

use tracing::debug;

/// Translation lookup table built from `global.ini`.
#[derive(Debug, Clone, Default)]
pub struct Translations {
    map: HashMap<Arc<str>, Arc<str>>,
}

impl Translations {
    /// Look up a localization key (e.g. "@ui_CIToggleMiningMode").
    ///
    /// Keys in the INI are stored without the `@` prefix.
    pub fn lookup(&self, key: &str) -> Option<Arc<str>> {
        let stripped = key.strip_prefix('@').unwrap_or(key);
        self.map.get(stripped).cloned()
    }

    /// Look up a key, falling back to humanizing the raw key.
    pub fn lookup_or_humanize(&self, key: &str) -> Arc<str> {
        self.lookup(key)
            .map(|label| strip_press_suffix(&label).into())
            .unwrap_or_else(|| humanize_label(key).into())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Parse the `global.ini` file contents (UTF-16 LE encoded) into a Translations table.
pub fn parse_global_ini(data: &[u8]) -> Translations {
    let text = decode_utf16le(data);

    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some((raw_key, value)) = line.split_once('=') {
            let raw_key = raw_key.trim();
            let value = value.trim();
            if raw_key.is_empty() {
                continue;
            }

            // Strip CryEngine platform suffixes (e.g. ",P" for PC, ",X" for Xbox).
            // The XML references keys without the suffix, so store under both forms.
            let base_key = strip_platform_suffix(raw_key);
            let value: Arc<str> = Arc::from(value);

            // Insert the base key (without suffix) — this is what XML lookups use.
            map.insert(Arc::from(base_key), value.clone());
            // Also insert the raw key in case anything looks up the suffixed form.
            if base_key != raw_key {
                map.insert(Arc::from(raw_key), value);
            }
        }
    }

    debug!("Loaded {} translation entries", map.len());
    Translations { map }
}

/// Decode UTF-16 LE bytes to a String, handling BOM.
fn decode_utf16le(data: &[u8]) -> String {
    let (cow, _, _) = encoding_rs::UTF_16LE.decode(data);
    let s = cow.into_owned();
    // Strip BOM if present
    s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string()
}

/// Strip CryEngine platform suffix from a localization key.
///
/// Examples: `"ui_foo,P"` → `"ui_foo"`, `"ui_bar"` → `"ui_bar"`
fn strip_platform_suffix(key: &str) -> &str {
    // Platform suffixes are a comma followed by uppercase letters at the end
    if let Some(pos) = key.rfind(',') {
        let suffix = &key[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_uppercase()) {
            return &key[..pos];
        }
    }
    key
}

/// Humanize a raw action/label key by stripping prefixes and splitting on boundaries.
///
/// Examples:
/// - `@ui_CGSeatGeneral` → "CG Seat General"
/// - `v_power_toggle` → "V Power Toggle"
pub fn humanize_label(raw: &str) -> String {
    let s = raw
        .strip_prefix('@')
        .unwrap_or(raw)
        .strip_prefix("ui_")
        .unwrap_or(raw.strip_prefix('@').unwrap_or(raw));

    let mut words = Vec::new();
    let mut current = String::new();

    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            // camelCase boundary: "SeatGeneral" -> "Seat", "General"
            let last_was_upper = current.chars().last().is_some_and(|c| c.is_uppercase());
            if !last_was_upper {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch);
        } else if ch.is_lowercase() && current.len() >= 2 {
            // Acronym-to-word boundary: "CGSeat" -> "CG", "Seat"
            let last_was_upper = current.chars().last().is_some_and(|c| c.is_uppercase());
            if last_was_upper {
                // Split: move the last uppercase char to start a new word
                let last = current.pop().unwrap();
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                current.push(last);
            }
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }

    words
        .iter()
        .map(|w| title_case(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip " (Short Press)" or " (Long Press)" suffixes from labels.
pub fn strip_press_suffix(label: &str) -> &str {
    label
        .strip_suffix(" (Short Press)")
        .or_else(|| label.strip_suffix(" (Long Press)"))
        .unwrap_or(label)
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = first.to_uppercase().to_string();
            for ch in chars {
                result.push(ch);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_ui_prefix() {
        assert_eq!(humanize_label("@ui_CGSeatGeneral"), "CG Seat General");
    }

    #[test]
    fn humanize_underscore() {
        assert_eq!(humanize_label("v_power_toggle"), "V Power Toggle");
    }

    #[test]
    fn humanize_plain() {
        assert_eq!(humanize_label("fire"), "Fire");
    }

    #[test]
    fn strip_press_suffixes() {
        assert_eq!(
            strip_press_suffix("Toggle Power (Short Press)"),
            "Toggle Power"
        );
        assert_eq!(
            strip_press_suffix("Toggle Power (Long Press)"),
            "Toggle Power"
        );
        assert_eq!(strip_press_suffix("Toggle Power"), "Toggle Power");
    }

    #[test]
    fn parse_ini_basic() {
        // Simulate UTF-16 LE content
        let text =
            "ui_CIToggleMiningMode=Toggle Mining Mode\nui_CGSpaceFlight=Spaceship - Flight\n";
        let bytes = encode_utf16le(text);
        let t = parse_global_ini(&bytes);
        assert_eq!(
            t.lookup("@ui_CIToggleMiningMode").unwrap().as_ref(),
            "Toggle Mining Mode"
        );
        assert_eq!(
            t.lookup("ui_CGSpaceFlight").unwrap().as_ref(),
            "Spaceship - Flight"
        );
    }

    #[test]
    fn parse_ini_platform_suffix() {
        let text = "ui_v_set_flight_mode,P=Set Flight Operator Mode\nui_plain=Plain Label\n";
        let bytes = encode_utf16le(text);
        let t = parse_global_ini(&bytes);
        // Lookup without suffix should work
        assert_eq!(
            t.lookup("@ui_v_set_flight_mode").unwrap().as_ref(),
            "Set Flight Operator Mode"
        );
        // Lookup with suffix should also work
        assert_eq!(
            t.lookup("ui_v_set_flight_mode,P").unwrap().as_ref(),
            "Set Flight Operator Mode"
        );
        // Plain key without suffix still works
        assert_eq!(t.lookup("ui_plain").unwrap().as_ref(), "Plain Label");
    }

    #[test]
    fn strip_platform_suffix_cases() {
        assert_eq!(super::strip_platform_suffix("ui_foo,P"), "ui_foo");
        assert_eq!(super::strip_platform_suffix("ui_bar,XB"), "ui_bar");
        assert_eq!(super::strip_platform_suffix("ui_baz"), "ui_baz");
        // Don't strip if suffix has lowercase
        assert_eq!(super::strip_platform_suffix("ui_qux,abc"), "ui_qux,abc");
    }

    fn encode_utf16le(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE]; // BOM
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        bytes
    }
}
