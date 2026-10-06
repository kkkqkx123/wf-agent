//! Literal color compatibility: map historical literal colors to semantic roles.

use super::data::{ColorRole, Rgb, Theme};

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
