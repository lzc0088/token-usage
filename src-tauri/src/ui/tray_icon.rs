//! Tray icon rasterizer — draws the "T" glyph and (optionally) usage text
//! into a single template bitmap.
//!
//! Tauri's tray API exposes no title-font control (macOS renders `set_title`
//! at the fixed system menu-bar size), and `set_title` is unsupported on
//! Windows entirely. Rendering text INTO the icon solves both: the OS scales
//! the whole bitmap to the menu-bar height, so a 32px-tall canvas yields
//! ~40% smaller text than the system 13pt, on every platform.
//!
//! Layout: `[T glyph 32px][3px gap][text]`, all on a 32px-tall canvas drawn
//! at 4× supersampling, then box-filtered down. RGB stays 0 with alpha-only
//! variation (template image) so macOS auto-adapts to light/dark menu bars.

use super::pixel_font;

/// Canvas height in final pixels; the OS scales this to the menu-bar height.
const H: usize = 32;
/// Supersampling factor.
const SF: usize = 4;
/// Text render scale — each font pixel becomes a 2×2 block (7px font rows →
/// 14px of the 32px canvas ≈ 44% of menu-bar height).
const TEXT_SCALE: usize = 2;
/// Transparent gap between the T glyph area and the text area (final px).
const GAP_PX: usize = 3;

/// Build the plain 32×32 template icon: rounded-rect border enclosing a bold
/// centred "T". The glyph sits inside a 3px transparent margin so it renders
/// small in the menu bar — the OS scales the full canvas to bar height
/// regardless, so the margin is what makes it look compact.
pub fn build_tray_icon() -> tauri::image::Image<'static> {
    build_icon_with_text("")
}

/// Build a `[T][text]` template icon. Empty `text` yields the plain icon.
pub fn build_icon_with_text(text: &str) -> tauri::image::Image<'static> {
    // Measure at the HIRES scale the text is actually drawn at, then ceil to
    // final pixels. Measuring at the final scale understates small-unit
    // chars (integer 3/4 rounding) and the drawn text overflows the canvas
    // right edge — the last characters get clipped off.
    let text_px = pixel_font::text_width(text, TEXT_SCALE * SF).div_ceil(SF);
    let w = if text_px == 0 {
        H
    } else {
        H + GAP_PX + text_px
    };
    let hw = w * SF; // hires width
    let hh = H * SF; // hires height (always 128)
    let mut hi = vec![0u8; hw * hh]; // alpha only

    // ── helper: is (fx, fy) inside a rounded rectangle? ────────────────
    fn in_rr(mut fx: f64, mut fy: f64, l: f64, t: f64, r: f64, b: f64, rad: f64) -> bool {
        // Mirror into the top-left quadrant relative to centre
        let cx = (l + r) * 0.5;
        let cy = (t + b) * 0.5;
        fx = (fx - cx).abs();
        fy = (fy - cy).abs();
        let hw = (r - l) * 0.5;
        let hh = (b - t) * 0.5;
        if fx > hw || fy > hh {
            return false;
        }
        // Corner: (fx, fy) is relative to centre in quadrant I
        let dx = (fx - (hw - rad)).max(0.0);
        let dy = (fy - (hh - rad)).max(0.0);
        dx * dx + dy * dy <= rad * rad
    }

    let sf = SF as f64;

    // Outer rounded rect: border starts at margin 3px, extends 2.5px thick
    let ol = 3.0 * sf;
    let ot = 3.0 * sf;
    let or_ = (H as f64 - 3.0) * sf;
    let ob = (H as f64 - 3.0) * sf;
    let rad_o = 6.0 * sf; // outer corner radius

    // Inner rounded rect (hole): border is 2.5px thick
    let il = (3.0 + 2.5) * sf;
    let it_ = (3.0 + 2.5) * sf;
    let ir = (H as f64 - 3.0 - 2.5) * sf;
    let ib = (H as f64 - 3.0 - 2.5) * sf;
    let rad_i = f64::max(6.0 - 2.5, 0.0) * sf; // inner radius

    // Draw border
    for y in 0..hh {
        for x in 0..(H * SF) {
            let fx = x as f64 + 0.5;
            let fy = y as f64 + 0.5;
            if in_rr(fx, fy, ol, ot, or_, ob, rad_o) && !in_rr(fx, fy, il, it_, ir, ib, rad_i) {
                hi[y * hw + x] = 255;
            }
        }
    }

    // Bold "T" centred inside the inner area
    let pad = 4.0 * sf;
    let tl = (il + pad) as usize;
    let tr = (ir - pad) as usize;
    let tt = (it_ + pad) as usize;
    let tb = (ib - pad) as usize;
    let tcx = (tl + tr) / 2;
    let bar_h = ((tb - tt) as f64 * 0.28) as usize; // thicker crossbar
    let stem_w = ((tr - tl) as f64 * 0.24) as usize; // thicker stem

    // Crossbar (top)
    for y in tt..tt + bar_h {
        for x in tl..tr {
            hi[y * hw + x] = 255;
        }
    }
    // Stem (centre, from crossbar to bottom)
    for y in tt..tb {
        for x in tcx - stem_w / 2..tcx + stem_w / 2 {
            hi[y * hw + x] = 255;
        }
    }

    // ── text region (hires coords) ──────────────────────────────────────
    if text_px > 0 {
        let x = (H + GAP_PX) * SF;
        let text_h = 7 * TEXT_SCALE; // final px
        let y = ((H - text_h) / 2) * SF; // vertically centred
        pixel_font::draw_text(&mut hi, hw, x, y, text, TEXT_SCALE * SF);
    }

    // ── down-sample SF× → w×H RGBA ─────────────────────────────────────
    downsample(&hi, hw, w, H)
}

/// Box-filter an SF×-supersampled alpha canvas down to a `w×h` RGBA template
/// image (RGB=0, alpha averaged per block).
fn downsample(hi: &[u8], hw: usize, w: usize, h: usize) -> tauri::image::Image<'static> {
    let mut rgba = vec![0u8; w * h * 4];
    let block = (SF * SF) as u32;
    for oy in 0..h {
        for ox in 0..w {
            let mut sum = 0u32;
            for dy in 0..SF {
                for dx in 0..SF {
                    sum += hi[(oy * SF + dy) * hw + (ox * SF + dx)] as u32;
                }
            }
            let a = (sum / block) as u8;
            let i = (oy * w + ox) * 4;
            rgba[i + 3] = a;
        }
    }
    tauri::image::Image::new_owned(rgba, w as u32, h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every title the formatters can emit must render without panic and
    /// with a sensible width (mirror of tray title outputs).
    const SAMPLE_TITLES: &[&str] = &[
        "2.8M/$4.2",
        "$4.2",
        "2.8M",
        "¥29.5",
        "¥29.5/$4.2",
        "2.8M/¥29.5/$4.2",
        "83%",
        "100%",
        "-3.2M",
        "1.2B",
    ];

    #[test]
    fn text_icon_wider_than_plain() {
        let plain = build_icon_with_text("");
        let with_text = build_icon_with_text("2.8M·$4.2");
        assert_eq!((plain.width(), plain.height()), (32, 32));
        assert!(
            with_text.width() > 32,
            "text icon must be wider, got {}",
            with_text.width()
        );
        assert_eq!(with_text.height(), 32);
    }

    #[test]
    fn empty_text_matches_plain_dimensions() {
        let a = build_icon_with_text("");
        assert_eq!((a.width(), a.height()), (32, 32));
    }

    #[test]
    fn text_icon_is_template_pixels() {
        // RGBA: RGB channels must all be 0 (template), alpha varies.
        let img = build_icon_with_text("83%");
        let rgba = img.rgba();
        for (i, b) in rgba.iter().enumerate() {
            let channel = i % 4;
            if channel < 3 {
                assert_eq!(*b, 0, "RGB channel {channel} at byte {i} must be 0");
            }
        }
        // Some alpha must be non-zero (something is drawn).
        assert!(
            rgba.iter().skip(3).step_by(4).any(|a| *a > 0),
            "icon must have visible pixels"
        );
    }

    #[test]
    fn all_title_chars_render() {
        let mut prev_width = 0usize;
        for title in SAMPLE_TITLES {
            let img = build_icon_with_text(title);
            assert_eq!(img.height(), 32);
            assert!(
                img.width() as usize > 32,
                "{title:?} should produce a text region"
            );
            // Roughly monotonic: longer titles → wider icons (not guaranteed
            // strictly, so only sanity-check the first).
            if prev_width == 0 {
                prev_width = img.width() as usize;
            }
        }
    }

    #[test]
    fn icon_width_covers_full_text_extent() {
        // The canvas must be wide enough for text drawn at the HIRES scale —
        // measuring at the final scale understates small-unit chars and clips
        // the last characters off the right edge.
        for title in SAMPLE_TITLES {
            let img = build_icon_with_text(title);
            let w = img.width() as usize;
            let need_hires = (H + GAP_PX) * SF + pixel_font::text_width(title, TEXT_SCALE * SF);
            assert!(
                need_hires <= w * SF,
                "canvas {w}px too narrow for {title:?}: needs {need_hires} hires px"
            );
        }
    }

    #[test]
    fn text_pixels_appear_right_of_icon_region() {
        // With text, some alpha must exist beyond x=35 (icon 32px + gap 3px)
        // at ANY row (text is vertically centred around rows 9-22).
        let img = build_icon_with_text("888");
        let w = img.width() as usize;
        let rgba = img.rgba();
        let lit = (35..w).any(|x| (0..32).any(|y| rgba[(y * w + x) * 4 + 3] > 0));
        assert!(lit, "text pixels must appear right of the icon+gap");
    }
}
