//! Theme derivation logic: luminance calculation, theme derivation from
//! background color, and color blending utilities.

use super::data::{Rgb, Theme, ThemeKind, ThemeSource};

/// Relative luminance in `0.0..=1.0` (ITU-R BT.601 weights).
pub fn luminance(c: Rgb) -> f32 {
    (0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32) / 255.0
}

/// Accent candidates; the winner maximizes contrast against the background.
const ACCENT_CANDIDATES: [Rgb; 4] = [
    Rgb::new(0x22, 0xD3, 0xEE), // cyan
    Rgb::new(0xA7, 0x8B, 0xFA), // violet
    Rgb::new(0xF5, 0x9E, 0x0B), // amber
    Rgb::new(0x2D, 0xD4, 0xBF), // teal
];

/// Luminance threshold below which a background is "dark".
const DARK_LUMINANCE_THRESHOLD: f32 = 0.5;

/// Derive a full theme from a background color (foreground defaults to the
/// classic white/black contrasting default when `fg` is `None`).
pub fn derive_theme(bg: Rgb, fg: Option<Rgb>) -> Theme {
    let kind = if luminance(bg) < DARK_LUMINANCE_THRESHOLD {
        ThemeKind::Dark
    } else {
        ThemeKind::Light
    };
    let default_fg = match kind {
        ThemeKind::Dark => Rgb::new(0xE5, 0xE7, 0xEB),
        ThemeKind::Light => Rgb::new(0x1F, 0x29, 0x37),
    };
    let fg = fg.unwrap_or(default_fg);

    let bg_lum = luminance(bg);
    let accent = ACCENT_CANDIDATES
        .into_iter()
        .max_by(|a, b| {
            let da = (luminance(*a) - bg_lum).abs();
            let db = (luminance(*b) - bg_lum).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(Rgb::new(0x22, 0xD3, 0xEE));

    // Role variants follow the theme kind (dark → brighter, light → deeper).
    let (add, remove, warning, error, highlight) = match kind {
        ThemeKind::Dark => (
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0xFA, 0xCC, 0x15),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0x60, 0xA5, 0xFA),
        ),
        ThemeKind::Light => (
            Rgb::new(0x15, 0x80, 0x3D),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0xA1, 0x62, 0x07),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0x1D, 0x4E, 0xD8),
        ),
    };

    Theme {
        kind,
        fg,
        bg,
        muted: fg.blend(bg, 0.55),
        accent,
        add,
        remove,
        warning,
        error,
        highlight,
        source: ThemeSource::Probed,
    }
}

/// Whether the background is light (true) or dark (false).
pub fn is_light(bg: Rgb) -> bool {
    luminance(bg) > DARK_LUMINANCE_THRESHOLD
}

/// Blend two RGB colors with alpha. `alpha = 0.0` → `top`, `alpha = 1.0` → `bottom`.
pub fn blend_rgb(top: Rgb, bottom: Rgb, alpha: f32) -> Rgb {
    top.blend(bottom, alpha)
}
