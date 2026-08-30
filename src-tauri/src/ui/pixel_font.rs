//! Minimal 5×7 pixel font for the tray icon bitmap.
//!
//! Tauri's tray API offers no control over title font size (macOS renders
//! `set_title` with the fixed system menu-bar font), so usage text is drawn
//! INTO the icon bitmap instead (`ui::tray_icon`). This module provides the
//! glyph table plus measure/draw helpers for exactly the character set the
//! tray titles can emit (see `ui::fmt` and `ui::tray::format_title`):
//! digits, `. K M B - $ ¥ / % ·` and space.
//!
//! All drawing targets an alpha-only canvas (0 = transparent, 255 = opaque),
//! matching the supersampled rasterizer in `ui::tray_icon`.

/// Glyph lookup. Each glyph is 7 rows × 5 columns; bit 4 (0b10000) is the
/// leftmost column. Unknown characters return `None` (skipped by callers).
pub fn glyph(ch: char) -> Option<&'static [u8; 7]> {
    Some(match ch {
        '0' => &[0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => &[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => &[0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => &[0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => &[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => &[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => &[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => &[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => &[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        'K' => &[0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'M' => &[0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'B' => &[0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        '-' => &[0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '$' => &[0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
        '¥' => &[0x11, 0x11, 0x0A, 0x04, 0x1F, 0x04, 0x1F],
        '/' => &[0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '%' => &[0x19, 0x1A, 0x02, 0x04, 0x08, 0x0B, 0x13],
        '·' => &[0x00, 0x00, 0x00, 0x0C, 0x0C, 0x00, 0x00],
        ' ' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => return None,
    })
}

/// Characters rendered at ~3/4 size, baseline-aligned with the digits —
/// quantity units and currency symbols read as suffixes rather than
/// competing with the number.
const SMALL_CHARS: [char; 7] = ['K', 'M', 'B', '%', '·', '$', '¥'];

/// Per-character render scale: units render at three-quarters of the base
/// scale (minimum 1). The supersampled canvas absorbs the non-integer final
/// size as antialiased edges.
fn char_scale(ch: char, base: usize) -> usize {
    if SMALL_CHARS.contains(&ch) {
        (base * 3 / 4).max(1)
    } else {
        base
    }
}

/// Rendered width of `s` at `scale` (each font pixel becomes a scale×scale
/// block; small-unit chars render at 3/4 scale). Only renderable characters
/// count; one `scale`-wide space between consecutive renderable characters,
/// none after the last.
pub fn text_width(s: &str, scale: usize) -> usize {
    let mut w = 0usize;
    let mut first = true;
    for ch in s.chars().filter(|c| glyph(*c).is_some()) {
        if !first {
            w += scale;
        }
        w += 5 * char_scale(ch, scale);
        first = false;
    }
    w
}

/// Draw `s` onto an alpha-only `canvas` (`canvas_w` pixels wide) with its
/// top-left at `(x, y)`. Small-unit chars render at 3/4 scale and
/// bottom-align (share the digit baseline). Unknown characters are skipped;
/// out-of-canvas pixels are clipped (no panics).
pub fn draw_text(canvas: &mut [u8], canvas_w: usize, x: usize, y: usize, s: &str, scale: usize) {
    let canvas_h = canvas.len() / canvas_w;
    let mut cx = x;
    for ch in s.chars() {
        let Some(g) = glyph(ch) else { continue };
        let cs = char_scale(ch, scale);
        // Bottom-align: the glyph's last row sits on the digits' last row.
        let y0 = y + 7 * scale - 7 * cs;
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5usize {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                for dy in 0..cs {
                    for dx in 0..cs {
                        let px = cx + col * cs + dx;
                        let py = y0 + row * cs + dy;
                        if px < canvas_w && py < canvas_h {
                            canvas[py * canvas_w + px] = 255;
                        }
                    }
                }
            }
        }
        cx += 5 * cs + scale; // glyph width + spacing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every string the tray title formatters can emit (mirror of
    /// `ui::tray::tests::format_examples` outputs plus quota_min and
    /// negative-token variants) must be fully renderable.
    const SAMPLE_TITLES: &[&str] = &[
        "2.8M/$4.2",
        "$4.2",
        "2.8M",
        "¥29.5",
        "¥29.5/$4.2",
        "2.8M/¥29.5/$4.2",
        "83%",
        "100%",
        "0%",
        "-3.2M",
        "1.2B",
        "999K",
        "123",
    ];

    #[test]
    fn glyph_covers_all_title_chars() {
        for title in SAMPLE_TITLES {
            for ch in title.chars() {
                assert!(
                    glyph(ch).is_some(),
                    "title char {ch:?} in {title:?} has no glyph"
                );
            }
        }
    }

    #[test]
    fn glyph_rejects_unknown_chars() {
        assert!(glyph('T').is_none());
        assert!(glyph('€').is_none());
        assert!(glyph('A').is_none());
    }

    #[test]
    fn text_width_matches_expected() {
        // Two normal chars: (5×2) + (1×2 spacing) + (5×2) = 22
        assert_eq!(text_width("12", 2), 22);
        // Unit char renders at half scale: 10+2+10+2+10+2+5 = 41
        assert_eq!(text_width("2.8M", 2), 41);
        // Single char at scale 1 = 5
        assert_eq!(text_width("1", 1), 5);
        // Empty / unknown-only → 0
        assert_eq!(text_width("", 2), 0);
        assert_eq!(text_width("TT", 2), 0);
        // Space still occupies a cell (5+1 spacing rhythm preserved)
        assert_eq!(text_width("1 2", 1), 5 + 1 + 5 + 1 + 5);
    }

    #[test]
    fn small_units_render_smaller_and_baseline_aligned() {
        // "1K" at scale 2 (cs = 2*3/4 = 1): '1' occupies rows 0-13, 'K' 7px
        // tall bottom-aligned → rows 7-13, starting at x = 10+2 = 12.
        const W: usize = 24;
        const H: usize = 14;
        let mut canvas = vec![0u8; W * H];
        draw_text(&mut canvas, W, 0, 0, "1K", 2);
        // K row 0 = 0x11 → cols 0 and 4 lit → px 12 and 16, at row 7.
        assert_eq!(canvas[7 * W + 12], 255, "K top row must start at y=7");
        // K bottom row (row 6 = 0x11) sits at row 13 — shares '1' baseline.
        assert_eq!(canvas[13 * W + 12], 255, "K must bottom-align with digits");
        // Above the K (row 6) nothing lit at its column.
        assert_eq!(canvas[6 * W + 12], 0, "K must not extend above y=7");
    }

    #[test]
    fn small_units_at_production_scale_are_three_quarters() {
        // Production hires scale is 8 (TEXT_SCALE 2 × SF 4): digits span
        // 7×8 = 56 rows; units at cs = 8*3/4 = 6 span 42 rows, starting at
        // row 56-42 = 14 — bigger than the old half size (28 rows).
        const W: usize = 64;
        const H: usize = 56;
        let mut canvas = vec![0u8; W * H];
        draw_text(&mut canvas, W, 0, 0, "1K", 8);
        // '1' is 5*8 = 40 wide + 8 spacing → K starts at x = 48.
        // K row 0 (0x11) → px 48 and 48+4*6 = 72... clipped; check px 48 at y=14.
        assert_eq!(
            canvas[14 * W + 48],
            255,
            "K top row at 3/4 scale starts at y=14"
        );
        assert_eq!(canvas[13 * W + 48], 0, "nothing above the K glyph");
        // K bottom (row 6) at 14+6*6 = 50 < 56 — shares the digit baseline.
        assert_eq!(
            canvas[50 * W + 48],
            255,
            "K bottom aligns with digit baseline"
        );
    }

    #[test]
    fn draw_text_paints_expected_pixels() {
        // '.' glyph = 0b01100 → bit4 is the leftmost column, so lit columns 1-2.
        const W: usize = 16;
        const H: usize = 10;
        let mut canvas = vec![0u8; W * H];
        draw_text(&mut canvas, W, 0, 0, ".", 1);
        // The four lit pixels of the 2×2 dot block (rows 5-6, cols 1-2).
        assert_eq!(canvas[5 * W + 1], 255);
        assert_eq!(canvas[5 * W + 2], 255);
        assert_eq!(canvas[6 * W + 1], 255);
        assert_eq!(canvas[6 * W + 2], 255);
        // A pixel that must stay clear (top-left of the glyph box).
        assert_eq!(canvas[0], 0);
    }

    #[test]
    fn draw_text_skips_unknown_and_clips_out_of_canvas() {
        let mut canvas = vec![0u8; 10 * 10];
        // Unknown chars skipped, far-off coords clipped — must not panic.
        draw_text(&mut canvas, 10, 8, 8, "T9", 2);
        // Only the '9' partially lands; something may or may not be lit, but
        // no panic and no index error is the assertion.
        draw_text(&mut canvas, 10, 0, 0, "1", 2);
        // '1' first row = 0b00100 → font column 2; at scale 2 the block spans
        // canvas cols 4-5 on rows 0-1.
        assert_eq!(canvas[4], 255);
        assert_eq!(canvas[5], 255);
        assert_eq!(canvas[10 + 4], 255); // row 1 also lit
        assert_eq!(canvas[0], 0); // col 0 stays clear
    }
}
