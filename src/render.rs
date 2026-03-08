use streamdeck_lib::prelude::*;
use streamdeck_render::{
    BorderStyle, Canvas, Color, FontHandle, TextOptions, VAlign, WrapOptions, measure_line,
    wrap_text,
};

use crate::state::fonts::FontsState;
use crate::styles::KeyStyle;

// ── Font Resolution ─────────────────────────────────────────────────────────────

/// Resolve a font for a given style: style.font → default.
fn resolve_font(style: &KeyStyle, cx: &Context) -> FontHandle {
    if let Some(fonts) = cx.try_ext::<FontsState>() {
        return fonts.resolve(&style.font);
    }
    crate::state::fonts::embedded_font()
}

// ── Auto-Scale Helper ───────────────────────────────────────────────────────────

/// Safety margin (px) added to measured text widths to account for glyph
/// sidebearing overshoot that `measure_line` / `wrap_text` may not capture.
const WIDTH_MARGIN: f32 = 4.0;

/// Step down from `size_max` to `size_min` in `size_step` increments until the
/// wrapped text fits within `max_lines`.  Returns `(chosen_size, wrapped_lines)`.
fn auto_scale(
    font: &FontHandle,
    text: &str,
    max_width: f32,
    max_lines: usize,
    size_max: f32,
    size_min: f32,
    size_step: f32,
) -> (f32, Vec<streamdeck_render::TextLine>) {
    let effective_width = max_width - WIDTH_MARGIN;
    let opts = WrapOptions {
        max_width: effective_width,
        max_lines,
    };
    let mut size = size_max;
    loop {
        let lines = wrap_text(font, size, text, &opts);
        let count_ok = lines.len() <= max_lines;
        // Also verify no single line overflows — wrap_text can't break
        // words smaller than max_width, so a long word may overflow.
        let width_ok = lines.iter().all(|l| l.width_px <= effective_width);
        if (count_ok && width_ok) || size <= size_min {
            return (size, lines);
        }
        size = (size - size_step).max(size_min);
    }
}

/// Step down from `size_max` to `size_min` until every line in `raw_lines` fits
/// within `max_width` as a single line.  Returns the chosen font size.
fn auto_scale_all_lines(
    font: &FontHandle,
    raw_lines: &[&str],
    max_width: f32,
    size_max: f32,
    size_min: f32,
    size_step: f32,
) -> f32 {
    let effective_width = max_width - WIDTH_MARGIN;
    let mut size = size_max;
    loop {
        let all_fit = raw_lines
            .iter()
            .all(|line| measure_line(font, size, line) <= effective_width);
        if all_fit || size <= size_min {
            return size;
        }
        size = (size - size_step).max(size_min);
    }
}

// ── Multi-Line Layout ───────────────────────────────────────────────────────────

const CANVAS_SIZE: f32 = 144.0;

/// Compute the first baseline and baseline-to-baseline spacing for a centered
/// block of `count` lines rendered at `font_size` with the given `line_height`
/// multiplier.  Returns `(first_baseline, line_spacing)`.
fn multiline_layout(count: usize, font_size: f32, line_height: f32) -> (f32, f32) {
    if count <= 1 {
        // Single line: vertically centered
        return (CANVAS_SIZE / 2.0 + font_size * 0.35, 0.0);
    }
    let spacing = font_size * line_height;
    let block_height = spacing * (count - 1) as f32;
    // Center the block; offset by 0.35 * font_size for baseline approximation
    let first = (CANVAS_SIZE - block_height) / 2.0 + font_size * 0.35;
    (first, spacing)
}

// ── Public Rendering Functions ──────────────────────────────────────────────────

/// Render a word-wrapped label on a key icon with the given style.
///
/// Used by ExecuteAction for action names.  Handles:
/// - Literal `\n` sequences from PI text fields → actual line breaks
/// - Text transforms (uppercase) and abbreviation from the style
/// - Continuous font auto-scaling to prevent clipping
pub fn render_label(cx: &Context, ctx_id: &str, text: &str, style: &KeyStyle) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();

    // 1. Replace literal \n (backslash + n from PI) with real newlines
    let text = text.replace("\\n", "\n");

    // 2. Apply text transform
    let text = match style.text_transform {
        crate::styles::TextTransform::Uppercase => text.to_uppercase(),
        crate::styles::TextTransform::None => text,
    };

    // 3. Apply abbreviation
    let text = if style.abbreviate {
        crate::abbreviations::abbreviate(&text)
    } else {
        text
    };

    // 4. Fill canvas + draw border (before text so Fill borders don't cover it)
    let (fill_color, border) = style.fill_and_border();
    canvas.fill(fill_color);
    canvas.draw_border(&border);

    // 5. Render — multi-line path if text contains newlines
    if text.contains('\n') {
        let raw_lines: Vec<&str> = text.split('\n').collect();
        let count = raw_lines.len();

        let chosen_size = auto_scale_all_lines(
            &font,
            &raw_lines,
            style.max_width,
            style.font_size_max,
            style.font_size_min,
            style.font_size_step,
        );

        let (first_baseline, line_spacing) =
            multiline_layout(count, chosen_size, style.line_height);

        for (i, line_text) in raw_lines.iter().enumerate() {
            let baseline = first_baseline + i as f32 * line_spacing;
            let wrapped = wrap_text(
                &font,
                chosen_size,
                line_text,
                &WrapOptions {
                    max_width: style.max_width,
                    max_lines: 1,
                },
            );
            if !wrapped.is_empty() {
                canvas
                    .draw_text(
                        &wrapped,
                        &TextOptions::new(font.clone(), chosen_size)
                            .color(style.text_color)
                            .v_align(VAlign::Baseline(baseline)),
                    )
                    .ok();
            }
        }
    } else {
        // Single-block auto-scaled text
        let (chosen_size, lines) = auto_scale(
            &font,
            &text,
            style.max_width,
            style.max_lines,
            style.font_size_max,
            style.font_size_min,
            style.font_size_step,
        );

        if !lines.is_empty() {
            canvas
                .draw_text(
                    &lines,
                    &TextOptions::new(font, chosen_size).color(style.text_color),
                )
                .ok();
        }
    }

    send_canvas(cx, ctx_id, canvas);
}

/// Render channel + version for ManageVersion Show mode.
pub fn render_channel(cx: &Context, ctx_id: &str, channel: &str, version: &str, style: &KeyStyle) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

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
                    .color(style.accent)
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
                    .color(style.dimmed)
                    .v_align(VAlign::Baseline(90.0)),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: style.accent,
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render a channel pin (ManageVersion Pin mode).
/// Active pins are bright, inactive pins are dimmed.
pub fn render_channel_pin(
    cx: &Context,
    ctx_id: &str,
    channel: &str,
    is_active: bool,
    style: &KeyStyle,
) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

    let text_color = if is_active {
        style.accent
    } else {
        style.dimmed
    };
    let border_color = if is_active {
        style.accent
    } else {
        style.dimmed.with_alpha(80)
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
                    .color(style.dimmed)
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
pub fn render_channel_cycle(
    cx: &Context,
    ctx_id: &str,
    current: &str,
    version: &str,
    next: &str,
    style: &KeyStyle,
) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

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
                    .color(style.accent)
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
                    .color(style.dimmed)
                    .v_align(VAlign::Baseline(60.0)),
            )
            .ok();
    }

    // Separator line — derived from dimmed color
    let separator = Color::rgba(
        style.dimmed.r / 2,
        style.dimmed.g / 2,
        style.dimmed.b / 2,
        style.dimmed.a,
    );
    canvas.draw_horizontal_line(72, separator);

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
                    .color(style.dimmed)
                    .v_align(VAlign::Baseline(100.0)),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: style.accent.with_alpha(120),
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render a progress/status message (e.g. "Loading…", "Generating…").
pub fn render_progress(cx: &Context, ctx_id: &str, text: &str, style: &KeyStyle) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

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
            .draw_text(&lines, &TextOptions::new(font, 18.0).color(style.dimmed))
            .ok();
    }

    send_canvas(cx, ctx_id, canvas);
}

/// Render a single centered line (auto-scaled to fit).
pub fn render_centered(cx: &Context, ctx_id: &str, text: &str, style: &KeyStyle) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

    let max_width = 136.0;
    let (chosen_size, lines) = auto_scale(&font, text, max_width, 1, 36.0, 10.0, 2.0);

    if !lines.is_empty() {
        canvas
            .draw_text(
                &lines,
                &TextOptions::new(font, chosen_size).color(style.text_color),
            )
            .ok();
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: style.accent,
    });
    send_canvas(cx, ctx_id, canvas);
}

/// Render multiple centered lines, each on its own row.
///
/// Splits `text` on `\n`, auto-scales to fit, and vertically distributes
/// lines evenly across the key face.
pub fn render_multiline(cx: &Context, ctx_id: &str, text: &str, style: &KeyStyle) {
    let font = resolve_font(style, cx);
    let mut canvas = Canvas::key_icon();
    canvas.fill(style.bg);

    let raw_lines: Vec<&str> = text.split('\n').collect();
    let count = raw_lines.len();

    if count == 0 {
        canvas.draw_border(&BorderStyle::Solid {
            thickness: 2.0,
            radius: 12.0,
            color: style.accent,
        });
        send_canvas(cx, ctx_id, canvas);
        return;
    }

    let max_width = 136.0;
    let chosen_size = auto_scale_all_lines(&font, &raw_lines, max_width, 28.0, 10.0, 2.0);

    let (first_baseline, line_spacing) = multiline_layout(count, chosen_size, style.line_height);

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
                        .color(style.text_color)
                        .v_align(VAlign::Baseline(baseline)),
                )
                .ok();
        }
    }

    canvas.draw_border(&BorderStyle::Solid {
        thickness: 2.0,
        radius: 12.0,
        color: style.accent,
    });
    send_canvas(cx, ctx_id, canvas);
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn send_canvas(cx: &Context, ctx_id: &str, canvas: Canvas) {
    if let Ok(data_url) = canvas.finish().to_data_url() {
        cx.sd().set_image(ctx_id, Some(data_url), None, None);
    }
}
