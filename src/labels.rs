//! Label processing pipeline.
//!
//! A `LabelMode` is an ordered sequence of `LabelStep`s applied to a label
//! string before rendering.  Steps range from simple text transforms (uppercase,
//! lowercase, title-case) to render-aware abbreviation (`FitAbbreviate`) and
//! user-defined replacements / regex substitutions.
//!
//! Built-in presets (`none`, `uppercase`, `smart`, `compact`) are always
//! available.  Users can add custom modes by placing TOML files in
//! `%APPDATA%/icu.veelume.starcitizen/labels/`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use streamdeck_render::{FontHandle, WrapOptions, wrap_text};

use crate::abbreviations::AbbreviationTable;

// ── Label Step ──────────────────────────────────────────────────────────────

/// A single transformation step in a label pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LabelStep {
    // Simple transforms
    Uppercase,
    Lowercase,
    TitleCase,

    /// Always abbreviate all matched words using the global abbreviation table.
    Abbreviate,

    /// Only abbreviate when the text won't fit at a readable font size.
    /// Progressively applies abbreviations (highest-priority first) until the
    /// text fits or all candidates are exhausted.
    FitAbbreviate,

    // Data-carrying transforms
    /// User-defined word-level replacements (case-insensitive matching).
    Replace {
        words: HashMap<String, String>,
    },

    /// Regex find/replace.
    Regex {
        pattern: String,
        with: String,
    },
}

// ── Label Mode ──────────────────────────────────────────────────────────────

/// A named label processing pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelMode {
    /// Unique identifier — set from filename stem when loaded from disk.
    #[serde(skip)]
    pub id: String,

    /// Human-readable display name shown in PI dropdowns.
    #[serde(default)]
    pub name: String,

    /// Ordered list of transform steps.
    #[serde(default)]
    pub steps: Vec<LabelStep>,
}

// ── Label Context ───────────────────────────────────────────────────────────

/// Render-related parameters needed by `FitAbbreviate`.
///
/// Simple steps ignore these fields; only `FitAbbreviate` uses them to measure
/// text against the available canvas space.
pub struct LabelContext<'a> {
    pub abbrevs: &'a AbbreviationTable,
    pub font: &'a FontHandle,
    pub max_width: f32,
    pub max_lines: usize,
    pub font_size_max: f32,
    pub font_size_min: f32,
    pub font_size_step: f32,
}

// ── Pipeline Execution ──────────────────────────────────────────────────────

/// Apply a label mode's pipeline to the given text.
pub fn apply(mode: &LabelMode, text: &str, lctx: &LabelContext) -> String {
    let mut result = text.to_string();
    for step in &mode.steps {
        result = apply_step(step, &result, lctx);
    }
    result
}

fn apply_step(step: &LabelStep, text: &str, lctx: &LabelContext) -> String {
    match step {
        LabelStep::Uppercase => text.to_uppercase(),
        LabelStep::Lowercase => text.to_lowercase(),
        LabelStep::TitleCase => title_case(text),
        LabelStep::Abbreviate => lctx.abbrevs.apply(text),
        LabelStep::FitAbbreviate => fit_abbreviate(text, lctx),
        LabelStep::Replace { words } => apply_replacements(text, words),
        LabelStep::Regex { pattern, with } => apply_regex(text, pattern, with),
    }
}

// ── Title Case ──────────────────────────────────────────────────────────────

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    let lower: String = chars.as_str().to_lowercase();
                    format!("{upper}{lower}")
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Replace ─────────────────────────────────────────────────────────────────

/// Apply user-defined word-level replacements (case-insensitive matching).
fn apply_replacements(text: &str, words: &HashMap<String, String>) -> String {
    if words.is_empty() {
        return text.to_string();
    }

    // Build a lookup table keyed by uppercase
    let lookup: HashMap<String, &str> = words
        .iter()
        .map(|(k, v)| (k.to_uppercase(), v.as_str()))
        .collect();

    text.split_whitespace()
        .map(|word| {
            let key = word.to_uppercase();
            match lookup.get(&key) {
                Some(replacement) => replacement.to_string(),
                None => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Regex ───────────────────────────────────────────────────────────────────

fn apply_regex(text: &str, pattern: &str, replacement: &str) -> String {
    match regex::Regex::new(pattern) {
        Ok(re) => re.replace_all(text, replacement).into_owned(),
        Err(e) => {
            tracing::warn!("Invalid regex pattern '{pattern}': {e}");
            text.to_string()
        }
    }
}

// ── Fit Abbreviate ──────────────────────────────────────────────────────────

/// Safety margin (px) added to measured text widths.
const WIDTH_MARGIN: f32 = 4.0;

/// Abbreviate progressively until text fits at a readable font size.
fn fit_abbreviate(text: &str, lctx: &LabelContext) -> String {
    let readable_threshold = (lctx.font_size_max + lctx.font_size_min) / 2.0;

    // 1. Check if the text already fits at a readable size
    let current_size = measure_fit_size(
        text,
        lctx.font,
        lctx.max_width,
        lctx.max_lines,
        lctx.font_size_max,
        lctx.font_size_min,
        lctx.font_size_step,
    );

    if current_size >= readable_threshold {
        return text.to_string();
    }

    // 2. Get abbreviation candidates sorted by priority desc, then savings desc
    let candidates = lctx.abbrevs.candidates(text);
    if candidates.is_empty() {
        return text.to_string();
    }

    // 3. Progressively apply candidates until text fits
    let mut applied: Vec<usize> = Vec::new();
    for (i, _candidate) in candidates.iter().enumerate() {
        applied.push(i);
        let abbreviated = lctx.abbrevs.apply_candidates(text, &candidates, &applied);

        let size = measure_fit_size(
            &abbreviated,
            lctx.font,
            lctx.max_width,
            lctx.max_lines,
            lctx.font_size_max,
            lctx.font_size_min,
            lctx.font_size_step,
        );

        if size >= readable_threshold {
            return abbreviated;
        }
    }

    // 4. Even fully abbreviated, still doesn't fit — return fully abbreviated
    lctx.abbrevs.apply_candidates(text, &candidates, &applied)
}

/// Determine the font size that auto-scaling would choose for the given text.
fn measure_fit_size(
    text: &str,
    font: &FontHandle,
    max_width: f32,
    max_lines: usize,
    size_max: f32,
    size_min: f32,
    size_step: f32,
) -> f32 {
    let effective_width = max_width - WIDTH_MARGIN;
    let opts = WrapOptions {
        max_width: effective_width,
        max_lines,
    };
    let mut size = size_max;
    loop {
        let lines = wrap_text(font, size, text, &opts);
        let count_ok = lines.len() <= max_lines;
        let width_ok = lines.iter().all(|l| l.width_px <= effective_width);
        if (count_ok && width_ok) || size <= size_min {
            return size;
        }
        size = (size - size_step).max(size_min);
    }
}

// ── Built-in Presets ────────────────────────────────────────────────────────

pub fn mode_none() -> LabelMode {
    LabelMode {
        id: "none".into(),
        name: "None".into(),
        steps: vec![],
    }
}

pub fn mode_uppercase() -> LabelMode {
    LabelMode {
        id: "uppercase".into(),
        name: "Uppercase".into(),
        steps: vec![LabelStep::Uppercase],
    }
}

pub fn mode_smart() -> LabelMode {
    LabelMode {
        id: "smart".into(),
        name: "Smart".into(),
        steps: vec![LabelStep::FitAbbreviate, LabelStep::Uppercase],
    }
}

pub fn mode_compact() -> LabelMode {
    LabelMode {
        id: "compact".into(),
        name: "Compact".into(),
        steps: vec![LabelStep::Abbreviate, LabelStep::Uppercase],
    }
}

/// All built-in label mode presets.
pub fn builtins() -> Vec<LabelMode> {
    vec![mode_none(), mode_uppercase(), mode_smart(), mode_compact()]
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_case_basic() {
        assert_eq!(title_case("hello world"), "Hello World");
        assert_eq!(title_case("TOGGLE POWER"), "Toggle Power");
        assert_eq!(title_case(""), "");
    }

    #[test]
    fn replace_case_insensitive() {
        let words: HashMap<String, String> = [
            ("quantum drive".into(), "QD".into()),
            ("retaliator".into(), "Reta".into()),
        ]
        .into();
        // Note: replace works word-by-word, so "quantum drive" as a two-word key
        // won't match word-by-word. Single-word replacements work.
        assert_eq!(apply_replacements("Retaliator", &words), "Reta");
        assert_eq!(apply_replacements("RETALIATOR", &words), "Reta");
    }

    #[test]
    fn regex_basic() {
        assert_eq!(
            apply_regex("v3.24.1 build", r"v\d+\.\d+\.\d+", ""),
            " build"
        );
    }

    #[test]
    fn regex_invalid_pattern_returns_original() {
        assert_eq!(apply_regex("test", r"[invalid", ""), "test");
    }
}
