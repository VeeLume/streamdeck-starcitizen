use std::sync::OnceLock;

use streamdeck_lib::prelude::*;
use streamdeck_render::{
    BorderStyle, Canvas, Color, FontHandle, FontRegistry, TextOptions, VAlign, WrapOptions,
    measure_line, wrap_text,
};

// ── Font ────────────────────────────────────────────────────────────────────────

static FONT: OnceLock<FontHandle> = OnceLock::new();

fn font() -> &'static FontHandle {
    FONT.get_or_init(|| {
        let mut reg = FontRegistry::new();
        reg.load_bytes(
            "mono",
            include_bytes!("../icu.veelume.starcitizen.sdPlugin/fonts/UAV-OSD-Sans-Mono.ttf"),
        )
        .expect("embedded font must load")
    })
}

// ── Colors ──────────────────────────────────────────────────────────────────────

const BG: Color = Color::rgba(20, 20, 35, 255);
const ACCENT: Color = Color::rgba(100, 140, 255, 255);
const DIMMED: Color = Color::rgba(120, 120, 140, 255);
const SEPARATOR: Color = Color::rgba(60, 60, 80, 255);

// ── Public Rendering Functions ──────────────────────────────────────────────────

/// Render a word-wrapped label on a key icon (used by ExecuteAction for action names).
pub fn render_label(cx: &Context, ctx_id: &str, text: &str, border: &BorderStyle) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    // Auto-scale: try decreasing sizes until text fits within 3 lines
    let sizes = [28.0, 24.0, 20.0, 16.0];
    let opts = WrapOptions {
        max_width: 130.0,
        max_lines: 3,
    };

    let mut chosen_size = sizes[sizes.len() - 1];
    let mut lines = wrap_text(&font, chosen_size, text, &opts);

    for &size in &sizes {
        let candidate = wrap_text(&font, size, text, &opts);
        if candidate.len() <= 3 {
            chosen_size = size;
            lines = candidate;
            break;
        }
    }

    if !lines.is_empty() {
        canvas
            .draw_text(
                &lines,
                &TextOptions::new(font, chosen_size).color(Color::WHITE),
            )
            .ok();
    }

    canvas.draw_border(border);
    send_canvas(cx, ctx_id, canvas);
}

/// Render channel + version for ManageVersion Show mode.
pub fn render_channel(cx: &Context, ctx_id: &str, channel: &str, version: &str) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    // Channel name large and centered
    let channel_lines = wrap_text(
        &font,
        32.0,
        channel,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !channel_lines.is_empty() {
        canvas
            .draw_text(
                &channel_lines,
                &TextOptions::new(font.clone(), 32.0)
                    .color(ACCENT)
                    .v_align(VAlign::Baseline(60.0)),
            )
            .ok();
    }

    // Version below, smaller
    let ver_lines = wrap_text(
        &font,
        20.0,
        version,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !ver_lines.is_empty() {
        canvas
            .draw_text(
                &ver_lines,
                &TextOptions::new(font, 20.0)
                    .color(DIMMED)
                    .v_align(VAlign::Baseline(90.0)),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: ACCENT,
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render a channel pin (ManageVersion Pin mode).
/// Active pins are bright, inactive pins are dimmed.
pub fn render_channel_pin(cx: &Context, ctx_id: &str, channel: &str, is_active: bool) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    let text_color = if is_active { ACCENT } else { DIMMED };
    let border_color = if is_active {
        ACCENT
    } else {
        DIMMED.with_alpha(80)
    };

    // "SET" label at top
    let set_lines = wrap_text(
        &font,
        16.0,
        "SET",
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !set_lines.is_empty() {
        canvas
            .draw_text(
                &set_lines,
                &TextOptions::new(font.clone(), 16.0)
                    .color(DIMMED)
                    .v_align(VAlign::Baseline(36.0)),
            )
            .ok();
    }

    // Channel name centered
    let ch_lines = wrap_text(
        &font,
        28.0,
        channel,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !ch_lines.is_empty() {
        canvas
            .draw_text(
                &ch_lines,
                &TextOptions::new(font, 28.0)
                    .color(text_color)
                    .v_align(VAlign::Baseline(82.0)),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: border_color,
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render the cycle view: current channel + version, separator, dimmed "→ NEXT".
pub fn render_channel_cycle(cx: &Context, ctx_id: &str, current: &str, version: &str, next: &str) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    // Current channel
    let cur_lines = wrap_text(
        &font,
        24.0,
        current,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !cur_lines.is_empty() {
        canvas
            .draw_text(
                &cur_lines,
                &TextOptions::new(font.clone(), 24.0)
                    .color(ACCENT)
                    .v_align(VAlign::Baseline(40.0)),
            )
            .ok();
    }

    // Version
    let ver_lines = wrap_text(
        &font,
        16.0,
        version,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !ver_lines.is_empty() {
        canvas
            .draw_text(
                &ver_lines,
                &TextOptions::new(font.clone(), 16.0)
                    .color(DIMMED)
                    .v_align(VAlign::Baseline(60.0)),
            )
            .ok();
    }

    // Separator line
    canvas.draw_horizontal_line(72, SEPARATOR);

    // "→ NEXT" label
    let next_label = format!("\u{2192} {next}");
    let next_lines = wrap_text(
        &font,
        18.0,
        &next_label,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 1,
        },
    );
    if !next_lines.is_empty() {
        canvas
            .draw_text(
                &next_lines,
                &TextOptions::new(font, 18.0)
                    .color(DIMMED)
                    .v_align(VAlign::Baseline(100.0)),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: ACCENT.with_alpha(120),
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render a progress/status message (e.g. "Loading…", "Generating…").
pub fn render_progress(cx: &Context, ctx_id: &str, text: &str) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    let lines = wrap_text(
        &font,
        18.0,
        text,
        &WrapOptions {
            max_width: 130.0,
            max_lines: 3,
        },
    );
    if !lines.is_empty() {
        canvas
            .draw_text(&lines, &TextOptions::new(font, 18.0).color(DIMMED))
            .ok();
    }

    send_canvas(cx, ctx_id, canvas);
}

/// Render a single centered line (auto-scaled to fit).
pub fn render_centered(cx: &Context, ctx_id: &str, text: &str) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    let sizes = [36.0, 28.0, 24.0, 20.0, 16.0];
    let max_width = 136.0;
    let mut chosen_size = sizes[sizes.len() - 1];

    for &size in &sizes {
        let width = measure_line(&font, size, text);
        if width <= max_width {
            chosen_size = size;
            break;
        }
    }

    let lines = wrap_text(
        &font,
        chosen_size,
        text,
        &WrapOptions {
            max_width,
            max_lines: 1,
        },
    );
    if !lines.is_empty() {
        canvas
            .draw_text(
                &lines,
                &TextOptions::new(font, chosen_size).color(Color::WHITE),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: ACCENT,
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render multiple centered lines, each on its own row.
///
/// Splits `text` on `\n`, auto-scales to fit, and vertically distributes
/// lines evenly across the key face.
pub fn render_multiline(cx: &Context, ctx_id: &str, text: &str) {
    let font = font().clone();
    let mut canvas = Canvas::key_icon();
    canvas.fill(BG);

    let raw_lines: Vec<&str> = text.split('\n').collect();
    let count = raw_lines.len();

    if count == 0 {
        canvas.draw_border(&BorderStyle::Solid {
            thickness: 2.0,
            radius: 12.0,
            color: ACCENT,
        });
        send_canvas(cx, ctx_id, canvas);
        return;
    }

    // Auto-scale: find largest size where all lines fit the width
    let max_width = 136.0;
    let sizes = [28.0, 24.0, 20.0, 16.0, 14.0];
    let mut chosen_size = sizes[sizes.len() - 1];

    for &size in &sizes {
        let all_fit = raw_lines
            .iter()
            .all(|line| measure_line(&font, size, line) <= max_width);
        if all_fit {
            chosen_size = size;
            break;
        }
    }

    // Distribute lines vertically with even spacing
    // Canvas is 144px; use ~16px top/bottom margin
    let total_height = 144.0 - 32.0; // usable height
    let line_spacing = if count == 1 {
        0.0
    } else {
        total_height / count as f32
    };

    // Center single line; distribute multiple lines
    let first_baseline = if count == 1 {
        72.0 + chosen_size * 0.35 // vertically centered
    } else {
        16.0 + line_spacing * 0.5 + chosen_size * 0.35
    };

    for (i, line_text) in raw_lines.iter().enumerate() {
        let baseline = first_baseline + i as f32 * line_spacing;
        let wrapped = wrap_text(
            &font,
            chosen_size,
            line_text,
            &WrapOptions {
                max_width,
                max_lines: 1,
            },
        );
        if !wrapped.is_empty() {
            canvas
                .draw_text(
                    &wrapped,
                    &TextOptions::new(font.clone(), chosen_size)
                        .color(Color::WHITE)
                        .v_align(VAlign::Baseline(baseline)),
                )
                .ok();
        }
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: ACCENT,
    });
    send_canvas(cx, ctx_id, canvas);
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn send_canvas(cx: &Context, ctx_id: &str, canvas: Canvas) {
    if let Ok(data_url) = canvas.finish().to_data_url() {
        cx.sd().set_image(ctx_id, Some(data_url), None, None);
    }
}
