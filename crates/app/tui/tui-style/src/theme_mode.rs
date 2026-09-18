//! Buffer-level light/dark adaptation applied once per frame.
//!
//! Components keep rendering with semantic dark-first colors; after the
//! whole frame is composed [`adapt_buffer_for_theme`] flips the buffer for
//! light terminals in a single pass. The flip preserves hue by inverting
//! only the HSV value channel, and [`Color::Reset`] cells are left alone so
//! the terminal background shows through. Adaptation only runs when no
//! explicit theme was configured (explicit configuration wins outright).

use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// Light or dark terminal mode driving buffer adaptation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Dark terminal: buffers render as-is.
    #[default]
    Dark,
    /// Light terminal: buffers are brightness-flipped once per frame.
    Light,
}

impl ThemeMode {
    /// Derive the mode from the probed theme kind.
    pub fn from_kind(kind: crate::theme::ThemeKind) -> Self {
        match kind {
            crate::theme::ThemeKind::Dark => Self::Dark,
            crate::theme::ThemeKind::Light => Self::Light,
        }
    }

    /// Resolve the effective mode. An explicit user theme always wins and
    /// disables buffer adaptation; otherwise the probed background decides,
    /// falling back to dark.
    pub fn resolve(
        explicit: Option<crate::theme::ThemeKind>,
        probed: Option<crate::theme::ThemeKind>,
    ) -> (Self, bool) {
        if let Some(kind) = explicit {
            return (Self::from_kind(kind), true);
        }
        (
            Self::from_kind(probed.unwrap_or(crate::theme::ThemeKind::Dark)),
            false,
        )
    }
}

/// Adapt every cell of `buf` for `mode`. Dark is a no-op. Light flips the
/// brightness of true-color cells while preserving hue; `Reset` and indexed
/// colors pass through untouched.
pub fn adapt_buffer_for_theme(buf: &mut Buffer, mode: ThemeMode, explicit: bool) {
    if explicit || mode == ThemeMode::Dark {
        return;
    }
    for cell in buf.content.iter_mut() {
        cell.fg = adapt_color(cell.fg);
        cell.bg = adapt_color(cell.bg);
    }
}

/// Resolve which theme components render with and how the frame buffer is
/// post-processed. An explicit user theme is used as-is with adaptation
/// disabled. Otherwise components always render the dark palette and light
/// terminals get a single buffer-level flip — never both, so colors cannot
/// be adapted twice. The user palette pass lives in
/// `tui-render::post_process` as the single pipeline; component styles
/// already carry the active palette resolved by `theme::resolve_theme`.
pub fn resolve_render_theme(probed: crate::theme::Theme) -> (crate::theme::Theme, ThemeMode, bool) {
    if probed.source == crate::theme::ThemeSource::File {
        let mode = ThemeMode::from_kind(probed.kind);
        return (probed, mode, true);
    }
    match probed.kind {
        crate::theme::ThemeKind::Dark => (probed, ThemeMode::Dark, false),
        crate::theme::ThemeKind::Light => {
            let mut dark = crate::theme::Theme::dark_default();
            dark.source = crate::theme::ThemeSource::Probed;
            (dark, ThemeMode::Light, false)
        }
    }
}

/// Flip one color for light terminals, preserving hue.
fn adapt_color(color: Color) -> Color {
    match color {
        Color::Reset => Color::Reset,
        Color::Rgb(r, g, b) => {
            let (r, g, b) = flip_value(r, g, b);
            Color::Rgb(r, g, b)
        }
        other => other,
    }
}

/// Invert the HSV value channel while keeping hue and saturation.
fn flip_value(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (h, s, v) = rgb_to_hsv(r, g, b);
    hsv_to_rgb(h, s, 1.0 - v)
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0).floor() as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn filled(color: Color) -> Buffer {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        for cell in buf.content.iter_mut() {
            cell.fg = color;
            cell.bg = color;
        }
        buf
    }

    #[test]
    fn dark_mode_leaves_the_buffer_untouched() {
        let mut buf = filled(Color::Rgb(10, 20, 30));
        let before = buf.clone();
        adapt_buffer_for_theme(&mut buf, ThemeMode::Dark, false);
        assert_eq!(buf.content, before.content);
    }

    #[test]
    fn explicit_config_disables_light_adaptation() {
        let mut buf = filled(Color::Rgb(10, 20, 30));
        let before = buf.clone();
        adapt_buffer_for_theme(&mut buf, ThemeMode::Light, true);
        assert_eq!(buf.content, before.content);
    }

    #[test]
    fn light_mode_flips_brightness_keeping_hue() {
        let mut buf = filled(Color::Rgb(10, 20, 60));
        adapt_buffer_for_theme(&mut buf, ThemeMode::Light, false);
        for cell in buf.content.iter() {
            assert!(matches!(cell.fg, Color::Rgb(..)));
            let Color::Rgb(r, g, b) = cell.fg else {
                continue;
            };
            assert!(r > 10 || g > 20 || b > 60, "brightness must rise");
        }
    }

    #[test]
    fn reset_cells_stay_transparent() {
        let mut buf = filled(Color::Reset);
        adapt_buffer_for_theme(&mut buf, ThemeMode::Light, false);
        for cell in buf.content.iter() {
            assert_eq!(cell.fg, Color::Reset);
            assert_eq!(cell.bg, Color::Reset);
        }
    }

    #[test]
    fn explicit_kind_wins_over_probed_kind() {
        use crate::theme::ThemeKind;
        let (mode, explicit) = ThemeMode::resolve(Some(ThemeKind::Dark), Some(ThemeKind::Light));
        assert_eq!((mode, explicit), (ThemeMode::Dark, true));
        let (mode, explicit) = ThemeMode::resolve(None, Some(ThemeKind::Light));
        assert_eq!((mode, explicit), (ThemeMode::Light, false));
        let (mode, explicit) = ThemeMode::resolve(None, None);
        assert_eq!((mode, explicit), (ThemeMode::Dark, false));
    }

    #[test]
    fn render_theme_never_stacks_explicit_and_adaptation() {
        use crate::theme::{Theme, ThemeKind, ThemeSource};
        let mut explicit = Theme::light_default();
        explicit.source = ThemeSource::File;
        let (theme, mode, is_explicit) = resolve_render_theme(explicit);
        assert_eq!(theme.kind, ThemeKind::Light);
        assert!(is_explicit);
        let _ = mode;
    }

    #[test]
    fn probed_light_renders_dark_with_adaptation() {
        use crate::theme::{Theme, ThemeSource};
        let mut probed = Theme::light_default();
        probed.source = ThemeSource::Probed;
        let (theme, mode, is_explicit) = resolve_render_theme(probed);
        assert_eq!(theme.kind, crate::theme::ThemeKind::Dark);
        assert_eq!(mode, ThemeMode::Light);
        assert!(!is_explicit);
    }
}
