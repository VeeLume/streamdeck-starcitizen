/// Key icon visual styles.
///
/// A `KeyStyle` describes the complete visual appearance of a rendered key:
/// background, text colors, border, font sizing, and text transforms.
///
/// Built-in presets (`default`, `sck3`) are always available.  Users can add
/// custom styles by placing JSON files in `%APPDATA%/icu.veelume.starcitizen/styles/`.
use serde::{Deserialize, Serialize};
use streamdeck_render::{BorderStyle, Color};

// ── Text Transform ───────────────────────────────────────────────────────────

/// Text transformation applied to labels before rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
}

// ── Border Definition ────────────────────────────────────────────────────────

/// Serializable border definition that maps to `streamdeck_render::BorderStyle`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BorderDef {
    #[default]
    None,
    Solid {
        thickness: f32,
        radius: f32,
        #[serde(with = "hex_color")]
        color: Color,
    },
    Vignette {
        width: f32,
        radius: f32,
        #[serde(with = "hex_color")]
        color: Color,
    },
    /// Filled rounded rectangle — renders as a thick SDF border that fills the
    /// entire interior.  Used for the SCK3 style.
    Fill { radius: f32 },
}

// ── Key Style ────────────────────────────────────────────────────────────────

/// Complete visual style for rendering a key icon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyStyle {
    /// Unique identifier (e.g. `"default"`, `"sck3"`, or a custom name).
    pub id: String,
    /// Human-readable display name shown in PI dropdowns.
    pub name: String,

    // ── Colors ───────────────────────────────────────────────────────────
    /// Background fill color.
    #[serde(with = "hex_color")]
    pub bg: Color,
    /// Primary text color.
    #[serde(with = "hex_color")]
    pub text_color: Color,
    /// Accent color (borders, highlights).
    #[serde(with = "hex_color")]
    pub accent: Color,
    /// Dimmed/secondary text color.
    #[serde(with = "hex_color")]
    pub dimmed: Color,

    // ── Border ───────────────────────────────────────────────────────────
    pub border: BorderDef,

    // ── Font sizing ──────────────────────────────────────────────────────
    pub font_size_max: f32,
    pub font_size_min: f32,
    pub font_size_step: f32,
    pub max_lines: usize,
    /// Maximum text width in pixels (canvas is 144px).
    pub max_width: f32,

    // ── Text layout ───────────────────────────────────────────────────────
    /// Line height as a multiplier of font size (baseline-to-baseline).
    /// E.g. `1.4` means baseline spacing = `font_size * 1.4`.
    /// The text block is vertically centered on the canvas.
    pub line_height: f32,

    // ── Text processing ──────────────────────────────────────────────────
    pub text_transform: TextTransform,
    /// Apply the abbreviation table before rendering.
    pub abbreviate: bool,

    // ── Font ────────────────────────────────────────────────────────────
    /// Font ID to use (empty = default embedded font).
    /// Must match a filename stem in `%APPDATA%/{PLUGIN_ID}/fonts/`.
    pub font: String,
}

impl Default for KeyStyle {
    fn default() -> Self {
        style_default()
    }
}

impl KeyStyle {
    /// Returns the canvas fill color and the `BorderStyle` to draw.
    ///
    /// For `BorderDef::Fill`, the canvas is filled with black and the border
    /// draws a thick rounded rectangle in `self.bg`.  For all other variants
    /// the canvas is filled with `self.bg` directly.
    pub fn fill_and_border(&self) -> (Color, BorderStyle) {
        match &self.border {
            BorderDef::None => (self.bg, BorderStyle::None),
            BorderDef::Solid {
                thickness,
                radius,
                color,
            } => (
                self.bg,
                BorderStyle::Solid {
                    thickness: *thickness,
                    radius: *radius,
                    color: *color,
                },
            ),
            BorderDef::Vignette {
                width,
                radius,
                color,
            } => (
                self.bg,
                BorderStyle::Vignette {
                    width: *width,
                    radius: *radius,
                    color: *color,
                },
            ),
            BorderDef::Fill { radius } => (
                Color::BLACK,
                BorderStyle::Solid {
                    thickness: 73.0, // > half of 144px → fills the entire rounded rect
                    radius: *radius,
                    color: self.bg,
                },
            ),
        }
    }
}

// ── Built-in Presets ─────────────────────────────────────────────────────────

pub fn style_default() -> KeyStyle {
    KeyStyle {
        id: "default".into(),
        name: "Default".into(),
        bg: Color::rgba(20, 20, 35, 255),
        text_color: Color::WHITE,
        accent: Color::rgba(100, 140, 255, 255),
        dimmed: Color::rgba(120, 120, 140, 255),
        border: BorderDef::None,
        font_size_max: 28.0,
        font_size_min: 10.0,
        font_size_step: 2.0,
        max_lines: 3,
        max_width: 130.0,
        line_height: 1.4,
        text_transform: TextTransform::None,
        abbreviate: false,
        font: String::new(),
    }
}

pub fn style_sck3() -> KeyStyle {
    // SCK3 Icon Pack: 288×288 transparent PNGs with a thin white border.
    // At 144px canvas scale: ~2px border, ~25px radius, ~2px inset.
    KeyStyle {
        id: "sck3".into(),
        name: "SCK3".into(),
        bg: Color::rgba(0, 0, 0, 0), // transparent — black key surface shows through
        text_color: Color::WHITE,
        accent: Color::WHITE,
        dimmed: Color::rgba(180, 180, 180, 255),
        border: BorderDef::Solid {
            thickness: 2.0,
            radius: 25.0,
            color: Color::WHITE,
        },
        font_size_max: 28.0,
        font_size_min: 10.0,
        font_size_step: 2.0,
        max_lines: 3,
        max_width: 120.0, // tighter for rounded corners
        line_height: 1.4,
        text_transform: TextTransform::Uppercase,
        abbreviate: true,
        font: String::new(),
    }
}

/// All built-in style presets.
pub fn builtins() -> Vec<KeyStyle> {
    vec![style_default(), style_sck3()]
}

// ── Hex Color Serde ──────────────────────────────────────────────────────────

mod hex_color {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use streamdeck_render::Color;

    pub fn serialize<S>(color: &Color, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if color.a == 255 {
            serializer.serialize_str(&format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b))
        } else {
            serializer.serialize_str(&format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                color.r, color.g, color.b, color.a
            ))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Color, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Color::from_hex(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid hex color: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_roundtrips() {
        let style = style_default();
        let json = serde_json::to_string_pretty(&style).unwrap();
        let parsed: KeyStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "default");
        assert_eq!(parsed.bg.r, 20);
    }

    #[test]
    fn sck3_style_roundtrips() {
        let style = style_sck3();
        let json = serde_json::to_string_pretty(&style).unwrap();
        let parsed: KeyStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "sck3");
        assert!(parsed.abbreviate);
        assert_eq!(parsed.text_transform, TextTransform::Uppercase);
    }

    #[test]
    fn sck3_border_is_thin_white_solid() {
        let style = style_sck3();
        let (fill, border) = style.fill_and_border();
        // Transparent bg → fill is transparent
        assert_eq!(fill.a, 0);
        match border {
            BorderStyle::Solid {
                thickness,
                radius,
                color,
            } => {
                assert!((thickness - 2.0).abs() < f32::EPSILON);
                assert!((radius - 25.0).abs() < f32::EPSILON);
                assert_eq!(color, Color::WHITE);
            }
            _ => panic!("expected Solid border"),
        }
    }

    #[test]
    fn fill_border_produces_thick_solid() {
        // The Fill variant is still available for custom styles
        let mut style = style_default();
        style.bg = Color::rgba(74, 74, 74, 255);
        style.border = BorderDef::Fill { radius: 16.0 };
        let (fill, border) = style.fill_and_border();
        assert_eq!(fill, Color::BLACK);
        match border {
            BorderStyle::Solid {
                thickness, radius, ..
            } => {
                assert!(thickness >= 73.0);
                assert_eq!(radius, 16.0);
            }
            _ => panic!("expected Solid border"),
        }
    }
}
