# Stream Deck Plugin Image Guidelines

## Quick Reference

| Image Type | Standard Size | High DPI Size | Format | Color Scheme |
|------------|---------------|---------------|--------|--------------|
| Plugin Icon | 256 × 256 px | 512 × 512 px | PNG | Full color |
| Category Icon | 28 × 28 px | 56 × 56 px | **SVG** preferred | Monochrome white |
| Action Icon | 20 × 20 px | 40 × 40 px | **SVG** preferred | Monochrome white |
| Key Icon (State) | 72 × 72 px | 144 × 144 px | **SVG**/PNG/GIF | Any |

## Format Preference

**SVG is strongly preferred** for all icons except the main plugin icon:
- Scales perfectly across all device resolutions
- Smaller file sizes
- Future-proof for new Stream Deck hardware

Only use PNG when SVG is not feasible (e.g., complex raster graphics, photos).

## Plugin Icon

**File:** `plugin.png` + `plugin@2x.png`
- Standard: 256 × 256 px / High DPI: 512 × 512 px
- Format: PNG only, full color allowed
- Must be recognizable at small sizes

## Category & Action Icons (List View)

- **MUST** use monochrome white (#FFFFFF) on transparent background
- **AVOID** colored icons or solid backgrounds (clashes with system theme)
- Minimum 2px stroke weight for legibility
- Category: 28×28 / 56×56 — Action: 20×20 / 40×40

## Key Icons (State Images)

- 72×72 standard / 144×144 high-DPI
- Full color allowed
- Consider dark backgrounds (Stream Deck buttons are typically dark)
- Limit programmatic updates to **10 per second maximum**
- Use states to reflect status: default, active, error

## High DPI Naming

PNG files use `@2x` suffix: `icon.png` (20×20) + `icon@2x.png` (40×40).
SVG files do **not** need `@2x` variants.

## Programmatic Rendering (streamdeck-render)

For custom fonts or dynamic text on key icons, use `streamdeck-render`:

```rust
let mut canvas = Canvas::key_icon();       // 144×144
let lines = wrap_text(&font, 28.0, "Label", &WrapOptions::default());
canvas.draw_text(&lines, &TextOptions::new(font, 28.0).color(Color::WHITE))?;
let b64 = canvas.finish().to_base64()?;
cx.sd().set_image_b64(ctx_id, b64);
```

## File Organization

```
images/
├── plugin.png            (256×256)
├── plugin@2x.png         (512×512)
├── category.svg          (28×28, mono white)
├── example.svg           (action icon, 20×20, mono white)
└── ...
```

## Resources

- [Elgato Image Guidelines](https://docs.elgato.com/guidelines/streamdeck/plugins/images-and-layouts)
- [Stream Deck SDK](https://docs.elgato.com/sdk/plugins/overview)
- [SVGOMG Optimizer](https://jakearchibald.github.io/svgomg/)
