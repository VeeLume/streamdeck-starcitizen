// Token-based fuzzy icon matching.
//
// Scores icon filenames against an action's ID and translated label
// to suggest the best icon from a user-provided folder.

/// Match icon filenames against an action's identity.
///
/// Returns up to 10 `(filename, score)` pairs sorted by descending score.
/// Label tokens are weighted 2× higher than action ID tokens.
pub fn match_icons(action_id: &str, label: &str, icon_files: &[String]) -> Vec<(String, f32)> {
    let id_tokens = tokenize(action_id);
    let label_tokens = tokenize(label);

    let mut scored: Vec<(String, f32)> = icon_files
        .iter()
        .map(|filename| {
            let file_tokens = tokenize(filename);
            let score = score_match(&id_tokens, &label_tokens, &file_tokens);
            (filename.clone(), score)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(10);
    scored
}

/// List icon files (SVG/PNG) from a directory.
pub fn list_icon_files(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".svg") || lower.ends_with(".png") {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

// ── Internals ────────────────────────────────────────────────────────────────────

fn tokenize(input: &str) -> Vec<String> {
    // Split on non-alphanumeric, underscores, hyphens, camelCase boundaries
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if ch.is_uppercase() && !current.is_empty() {
                let last_was_lower = current.chars().last().is_some_and(|c| c.is_lowercase());
                if last_was_lower {
                    tokens.push(std::mem::take(&mut current).to_lowercase());
                }
            }
            current.push(ch);
        } else {
            // Separator character
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current).to_lowercase());
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    tokens.retain(|t| t.len() >= 2 && seen.insert(t.clone()));
    tokens
}

fn score_match(id_tokens: &[String], label_tokens: &[String], file_tokens: &[String]) -> f32 {
    let mut score = 0.0f32;

    for ft in file_tokens {
        // Exact matches
        for lt in label_tokens {
            if ft == lt {
                score += 2.0; // label tokens weighted higher
            } else if ft.starts_with(lt) || lt.starts_with(ft) {
                score += 1.0;
            }
        }
        for it in id_tokens {
            if ft == it {
                score += 1.0;
            } else if ft.starts_with(it) || it.starts_with(ft) {
                score += 0.5;
            }
        }
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_action_name() {
        let tokens = tokenize("v_power_toggle");
        assert_eq!(tokens, vec!["power", "toggle"]);
    }

    #[test]
    fn tokenize_camel_case() {
        let tokens = tokenize("togglePowerOn");
        assert_eq!(tokens, vec!["toggle", "power", "on"]);
    }

    #[test]
    fn tokenize_filename() {
        let tokens = tokenize("power-toggle.svg");
        assert_eq!(tokens, vec!["power", "toggle", "svg"]);
    }

    #[test]
    fn match_scores_label_higher() {
        let files = vec![
            "power-toggle.svg".to_string(),
            "shield-toggle.svg".to_string(),
            "unrelated.svg".to_string(),
        ];

        let results = match_icons("v_power_toggle", "Toggle Power", &files);
        assert!(!results.is_empty());
        // power-toggle should score highest (matches both id and label tokens)
        assert_eq!(results[0].0, "power-toggle.svg");
    }

    #[test]
    fn empty_input_returns_empty() {
        let files = vec!["icon.svg".to_string()];
        let results = match_icons("", "", &files);
        assert!(results.is_empty());
    }

    #[test]
    fn no_match_returns_empty() {
        let files = vec!["completely-unrelated.svg".to_string()];
        let results = match_icons("v_power_toggle", "Toggle Power", &files);
        assert!(results.is_empty());
    }
}
