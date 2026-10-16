//! Literal color compatibility: map historical literal colors to semantic roles.
//!
//! True-color literals adsorb to their nearest role within [`FAMILY_RADIUS`]
//! and re-express relative to the active palette, preserving lightness and
//! chroma offsets. Colors outside every role family keep their original
//! value instead of snapping to an unrelated role. Indexed and named colors
//! follow the same adsorb path: indexed entries quantize to RGB first, named
//! entries map through their semantic role.

use std::collections::HashMap;

use ratatui::style::Color;

use super::data::{ColorRole, Rgb, Theme};
use super::oklab::{oklab_distance, oklab_to_rgb, rgb_to_oklab};

/// Maximum perceptual distance for family adsorption. Literals farther than
/// this from every role anchor keep their original color.
pub const FAMILY_RADIUS: f32 = 0.15;

/// Nearest semantic role for a historical literal color, measured in OKLab
/// against the dark-default palette (the palette every literal was picked
/// from). Returns the role whose default color is closest to `literal`;
/// exact ties resolve to the earlier role in palette order.
pub fn nearest_role(literal: Rgb) -> ColorRole {
    nearest_role_with_distance(literal).0
}

/// Nearest role plus its perceptual distance to the anchor.
pub fn nearest_role_with_distance(literal: Rgb) -> (ColorRole, f32) {
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
    let mut best = (ColorRole::Default, f32::MAX);
    for (role, color) in candidates {
        let distance = oklab_distance(literal, color);
        if distance < best.1 {
            best = (role, distance);
        }
    }
    best
}

/// True when `literal` sits inside a role family and may adsorb.
pub fn within_family(literal: Rgb) -> bool {
    nearest_role_with_distance(literal).1 <= FAMILY_RADIUS
}

/// Compatibility mapping for historical literal colors: resolve `literal`
/// to its nearest semantic role, then return that role's color from
/// `theme`. When the theme is untouched the result equals the historical
/// value, so default output stays byte-identical; user overrides flow to
/// every migrated call site without touching draw code. Literals outside
/// every family keep their original color.
pub fn remap_literal(literal: Rgb, theme: &Theme) -> Rgb {
    let (role, distance) = nearest_role_with_distance(literal);
    if distance > FAMILY_RADIUS {
        return literal;
    }
    theme.rgb_for_role(role)
}

/// Relative remap preserving the literal's OKLab offset from its role
/// anchor: the delta between the literal and the dark-default role color is
/// re-applied on top of the active theme role color. Exact role colors map
/// exactly (zero delta keeps default output byte-identical) while tinted
/// literals keep their lightness/chroma shift under a custom palette.
/// Out-of-family literals keep their original color instead of snapping.
pub fn remap_literal_relative(literal: Rgb, theme: &Theme) -> Rgb {
    let role = nearest_role(literal);
    let anchor = Theme::dark_default().rgb_for_role(role);
    if oklab_distance(literal, anchor) > FAMILY_RADIUS {
        return literal;
    }
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

/// Semantic role for a named terminal color. Bright variants share their
/// base role; black and white map to default text, grays to muted.
pub fn named_color_role(color: &Color) -> Option<ColorRole> {
    Some(match color {
        Color::Black => ColorRole::Default,
        Color::Red | Color::LightRed => ColorRole::Error,
        Color::Green | Color::LightGreen => ColorRole::Add,
        Color::Yellow | Color::LightYellow => ColorRole::Warning,
        Color::Blue | Color::LightBlue => ColorRole::Highlight,
        Color::Magenta | Color::LightMagenta => ColorRole::Highlight,
        Color::Cyan | Color::LightCyan => ColorRole::Accent,
        Color::Gray | Color::DarkGray => ColorRole::Muted,
        Color::White => ColorRole::Default,
        _ => return None,
    })
}

/// Convert an xterm 256-color index to RGB following the standard palette:
/// 0-7 system, 8-15 bright, 16-231 6x6x6 cube, 232-255 grayscale ramp.
pub fn indexed_to_rgb(index: u8) -> Rgb {
    const SYSTEM: [Rgb; 16] = [
        Rgb::new(0x00, 0x00, 0x00),
        Rgb::new(0x80, 0x00, 0x00),
        Rgb::new(0x00, 0x80, 0x00),
        Rgb::new(0x80, 0x80, 0x00),
        Rgb::new(0x00, 0x00, 0x80),
        Rgb::new(0x80, 0x00, 0x80),
        Rgb::new(0x00, 0x80, 0x80),
        Rgb::new(0xc0, 0xc0, 0xc0),
        Rgb::new(0x80, 0x80, 0x80),
        Rgb::new(0xff, 0x00, 0x00),
        Rgb::new(0x00, 0xff, 0x00),
        Rgb::new(0xff, 0xff, 0x00),
        Rgb::new(0x00, 0x00, 0xff),
        Rgb::new(0xff, 0x00, 0xff),
        Rgb::new(0x00, 0xff, 0xff),
        Rgb::new(0xff, 0xff, 0xff),
    ];
    if index < 16 {
        return SYSTEM[usize::from(index)];
    }
    if index >= 232 {
        let step = index.saturating_sub(232);
        let level = 8u8.saturating_add(step.saturating_mul(10));
        return Rgb::new(level, level, level);
    }
    let cube = index.saturating_sub(16);
    let channel = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    Rgb::new(
        channel(cube / 36),
        channel((cube % 36) / 6),
        channel(cube % 6),
    )
}

/// Remap one indexed color through quantization plus the shared adsorb path.
pub fn remap_indexed(index: u8, theme: &Theme) -> Rgb {
    remap_literal_relative(indexed_to_rgb(index), theme)
}

/// Remap one terminal color (true-color, indexed or named) to the active
/// palette. `Reset` passes through so transparency survives.
pub fn remap_color(color: Color, theme: &Theme) -> Color {
    match color {
        Color::Reset => Color::Reset,
        Color::Rgb(r, g, b) => {
            let mapped = remap_literal_relative(Rgb::new(r, g, b), theme);
            Color::Rgb(mapped.r, mapped.g, mapped.b)
        }
        Color::Indexed(index) => {
            let mapped = remap_indexed(index, theme);
            Color::Rgb(mapped.r, mapped.g, mapped.b)
        }
        named => {
            if let Some(role) = named_color_role(&named) {
                let mapped = theme.rgb_for_role(role);
                Color::Rgb(mapped.r, mapped.g, mapped.b)
            } else {
                named
            }
        }
    }
}

/// Stable 32-bit key for one terminal color, used by the per-frame memo
/// cache without requiring `Color: Hash`.
fn color_key(color: &Color) -> u32 {
    match *color {
        Color::Reset => 0,
        Color::Rgb(r, g, b) => {
            0x01_00_00_00 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
        }
        Color::Indexed(i) => 0x02_00_00_00 | u32::from(i),
        Color::Black => 0x03_00_00_01,
        Color::Red => 0x03_00_00_02,
        Color::Green => 0x03_00_00_03,
        Color::Yellow => 0x03_00_00_04,
        Color::Blue => 0x03_00_00_05,
        Color::Magenta => 0x03_00_00_06,
        Color::Cyan => 0x03_00_00_07,
        Color::Gray => 0x03_00_00_08,
        Color::DarkGray => 0x03_00_00_09,
        Color::LightRed => 0x03_00_00_0A,
        Color::LightGreen => 0x03_00_00_0B,
        Color::LightYellow => 0x03_00_00_0C,
        Color::LightBlue => 0x03_00_00_0D,
        Color::LightMagenta => 0x03_00_00_0E,
        Color::LightCyan => 0x03_00_00_0F,
        Color::White => 0x03_00_00_10,
    }
}

/// Per-frame memo cache for palette remaps. One frame touches the same role
/// colors thousands of times; the cache keeps the adsorb math off the hot
/// path. Cleared at the frame start, never shared across frames.
#[derive(Debug, Clone, Default)]
pub struct FrameColorCache {
    entries: HashMap<u32, Color>,
    hits: u64,
    misses: u64,
}

impl FrameColorCache {
    /// Empty cache for one frame.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remap with memoization.
    pub fn remap(&mut self, color: Color, theme: &Theme) -> Color {
        let key = color_key(&color);
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            return *cached;
        }
        self.misses += 1;
        let mapped = remap_color(color, theme);
        self.entries.insert(key, mapped);
        mapped
    }

    /// Cached entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hits this frame.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache misses this frame.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Clear for the next frame.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
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
        let tinted = remap_literal_relative(Rgb::new(0x30, 0xE0, 0xF5), &custom);
        assert_ne!(tinted, Rgb::new(0x11, 0x22, 0x33));
    }

    #[test]
    fn out_of_family_colors_keep_their_value() {
        let custom = Theme::dark_default();
        let far = Rgb::new(0x7F, 0x00, 0x7F);
        let (role, distance) = nearest_role_with_distance(far);
        let _ = role;
        if distance > FAMILY_RADIUS {
            assert_eq!(remap_literal_relative(far, &custom), far);
            assert_eq!(remap_literal(far, &custom), far);
        } else {
            assert!(within_family(far));
        }
        let magenta = Rgb::new(0xFF, 0x00, 0xFF);
        assert_eq!(remap_literal_relative(magenta, &custom), magenta);
    }

    #[test]
    fn indexed_quantizes_then_adsorbs() {
        let mut custom = Theme::dark_default();
        custom.accent = Rgb::new(0x11, 0x22, 0x33);
        let mapped = remap_indexed(14, &custom);
        assert_ne!(mapped, indexed_to_rgb(14));
        assert_ne!(mapped, Rgb::new(0x22, 0xD3, 0xEE));
        assert_eq!(indexed_to_rgb(0), Rgb::new(0, 0, 0));
        assert_eq!(indexed_to_rgb(15), Rgb::new(255, 255, 255));
        assert_eq!(indexed_to_rgb(16), Rgb::new(0, 0, 0));
    }

    #[test]
    fn named_colors_map_to_roles() {
        assert_eq!(named_color_role(&Color::Red), Some(ColorRole::Error));
        assert_eq!(named_color_role(&Color::Green), Some(ColorRole::Add));
        assert_eq!(named_color_role(&Color::Yellow), Some(ColorRole::Warning));
        assert_eq!(named_color_role(&Color::Blue), Some(ColorRole::Highlight));
        assert_eq!(named_color_role(&Color::Cyan), Some(ColorRole::Accent));
        assert_eq!(named_color_role(&Color::White), Some(ColorRole::Default));
        assert_eq!(named_color_role(&Color::Reset), None);
    }

    #[test]
    fn remap_color_covers_every_domain() {
        let mut custom = Theme::dark_default();
        custom.error = Rgb::new(0x11, 0x22, 0x33);
        custom.remove = Rgb::new(0x11, 0x22, 0x33);
        assert_eq!(remap_color(Color::Reset, &custom), Color::Reset);
        assert_eq!(
            remap_color(Color::Red, &custom),
            Color::Rgb(0x11, 0x22, 0x33)
        );
        let lit = indexed_to_rgb(9);
        let anchor = Theme::dark_default().rgb_for_role(ColorRole::Remove);
        let dist = super::super::oklab::oklab_distance(lit, anchor);
        assert!(dist <= super::FAMILY_RADIUS);
        let indexed = remap_color(Color::Indexed(9), &custom);
        assert_ne!(indexed, Color::Rgb(0xFF, 0x00, 0x00));
        assert_ne!(indexed, Color::Rgb(0xF8, 0x71, 0x71));
    }

    #[test]
    fn frame_cache_memoizes_repeated_colors() {
        let theme = Theme::dark_default();
        let mut cache = FrameColorCache::new();
        let first = cache.remap(Color::Rgb(0x22, 0xD3, 0xEE), &theme);
        let second = cache.remap(Color::Rgb(0x22, 0xD3, 0xEE), &theme);
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.len(), 1);
    }
}
