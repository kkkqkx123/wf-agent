//! Literal color compatibility: map historical literal colors to semantic roles.

use super::data::{ColorRole, Rgb, Theme};
use super::oklab::{oklab_to_rgb, rgb_to_oklab};

/// Squared Euclidean distance between two colors.
fn color_distance_sq(a: Rgb, b: Rgb) -> u32 {
    let dr = a.r as i32 - b.r as i32;
    let dg = a.g as i32 - b.g as i32;
    let db = a.b as i32 - b.b as i32;
    (dr * dr + dg * dg + db * db) as u32
}

/// Nearest semantic role for a historical literal color, measured against
/// the dark-default palette (the palette every literal was picked from).
/// Returns the role whose default color is closest to `literal`; exact ties
/// resolve to the earlier role in palette order.
pub fn nearest_role(literal: Rgb) -> ColorRole {
    let defaults = Theme::dark_default();
    let candidates = [
        (ColorRole::Default, defaults.fg),
        (ColorRole::Muted, defaults.muted),
        (ColorRole::Accent, defaults.accent),
        (ColorRole::Add, defaults.add),
        (ColorRole::Remove, defaults.remove),
        (ColorRole::Warning, defaults.warning),
        (ColorRole::Error, defaults.error),
        (ColorRole::Highlight, defaults.highlight),
    ];
    candidates
        .into_iter()
        .min_by_key(|(_, color)| color_distance_sq(literal, *color))
        .map(|(role, _)| role)
        .unwrap_or(ColorRole::Default)
}

/// Compatibility mapping for historical literal colors: resolve `literal`
/// to its nearest semantic role, then return that role's color from
/// `theme`. When the theme is untouched the result equals the historical
/// value, so default output stays byte-identical; user overrides flow to
/// every migrated call site without touching draw code.
pub fn remap_literal(literal: Rgb, theme: &Theme) -> Rgb {
    theme.rgb_for_role(nearest_role(literal))
}

/// Relative remap preserving the literal's OKLab offset from its role
/// anchor: the delta between the literal and the dark-default role color is
/// re-applied on top of the active theme role color. Exact role colors map
/// exactly (zero delta keeps default output byte-identical) while tinted
/// literals keep their lightness/chroma shift under a custom palette.
pub fn remap_literal_relative(literal: Rgb, theme: &Theme) -> Rgb {
    let role = nearest_role(literal);
    let anchor = Theme::dark_default().rgb_for_role(role);
    if literal == anchor {
        return theme.rgb_for_role(role);
    }
    let literal_lab = rgb_to_oklab(literal);
    let anchor_lab = rgb_to_oklab(anchor);
    let target_lab = rgb_to_oklab(theme.rgb_for_role(role));
    oklab_to_rgb(super::oklab::Oklab {
        l: target_lab.l + (literal_lab.l - anchor_lab.l),
        a: target_lab.a + (literal_lab.a - anchor_lab.a),
        b: target_lab.b + (literal_lab.b - anchor_lab.b),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_remap_is_identity_on_defaults() {
        let defaults = Theme::dark_default();
        for literal in [
            Rgb::new(0x22, 0xD3, 0xEE),
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xE5, 0xE7, 0xEB),
        ] {
            assert_eq!(remap_literal_relative(literal, &defaults), literal);
        }
    }

    #[test]
    fn relative_remap_preserves_tint_offset() {
        let mut custom = Theme::dark_default();
        custom.accent = Rgb::new(0x11, 0x22, 0x33);
        let exact = remap_literal_relative(Rgb::new(0x22, 0xD3, 0xEE), &custom);
        assert_eq!(exact, Rgb::new(0x11, 0x22, 0x33));
        // A tinted literal keeps a nonzero shift instead of collapsing.
        let tinted = remap_literal_relative(Rgb::new(0x30, 0xE0, 0xF5), &custom);
        assert_ne!(tinted, Rgb::new(0x11, 0x22, 0x33));
    }
}
