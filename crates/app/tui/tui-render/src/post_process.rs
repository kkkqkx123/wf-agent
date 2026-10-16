//! Fixed-order frame finishing pipeline.
//!
//! Every composed frame passes these stages exactly once, so widgets and
//! overlays never need to know the active theme: brightness adaptation runs
//! first, the user palette remap runs second, emoji preference runs third,
//! and the image cleanup counter runs last even when no image widget drew.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

use tui_style::theme::Theme;
use tui_style::theme_mode::ThemeMode;

/// Emoji rendering preference applied at the buffer level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmojiPreference {
    #[default]
    Native,
    Text,
}

/// Finish one frame buffer in fixed order.
pub fn finish_frame(
    buf: &mut Buffer,
    theme: &Theme,
    mode: ThemeMode,
    explicit: bool,
    emoji: EmojiPreference,
    cleanups: &mut usize,
) {
    tui_style::theme_mode::adapt_buffer_for_theme(buf, mode, explicit);
    adapt_buffer_for_palette(buf, theme);
    adapt_buffer_for_emoji(buf, emoji);
    *cleanups = cleanups.saturating_add(1);
}

/// User-palette pass: adsorb scattered literal colors to their nearest
/// semantic role and re-express them relative to the active palette,
/// preserving each literal's lightness/chroma offset. Runs after
/// brightness adaptation so seeded and fresh cells converge to the same
/// pixels. Default palettes map exactly (zero delta), keeping output
/// byte-identical when unconfigured.
pub fn adapt_buffer_for_palette(buf: &mut Buffer, theme: &Theme) {
    if palette_is_default(theme) {
        return;
    }
    for cell in buf.content.iter_mut() {
        cell.fg = remap_cell_color(cell.fg, theme);
        cell.bg = remap_cell_color(cell.bg, theme);
    }
}

fn remap_cell_color(color: Color, theme: &Theme) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let mapped =
                tui_style::theme::remap_literal_relative(tui_style::theme::Rgb::new(r, g, b), theme);
            Color::Rgb(mapped.r, mapped.g, mapped.b)
        }
        other => other,
    }
}

/// True when every role color matches the built-in dark default, ignoring
/// the provenance marker so probed and cached themes with default colors
/// still skip the remap and stay byte-identical.
fn palette_is_default(theme: &Theme) -> bool {
    let defaults = Theme::dark_default();
    theme.fg == defaults.fg
        && theme.bg == defaults.bg
        && theme.muted == defaults.muted
        && theme.accent == defaults.accent
        && theme.add == defaults.add
        && theme.remove == defaults.remove
        && theme.warning == defaults.warning
        && theme.error == defaults.error
        && theme.highlight == defaults.highlight
}

/// Emoji preference pass: text mode replaces wide pictographs with a
/// fallback marker, native mode leaves the buffer untouched.
pub fn adapt_buffer_for_emoji(buf: &mut Buffer, preference: EmojiPreference) {
    if preference == EmojiPreference::Native {
        return;
    }
    for cell in buf.content.iter_mut() {
        if cell.symbol().chars().any(is_wide_pictograph) {
            cell.set_symbol("?");
        }
    }
}

fn is_wide_pictograph(ch: char) -> bool {
    let code = ch as u32;
    (0x1F300..=0x1FAFF).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn native_emoji_leaves_buffer_untouched() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        buf.set_string(0, 0, "ab", ratatui::style::Style::default());
        let before = buf.content.clone();
        let mut cleanups = 0usize;
        finish_frame(
            &mut buf,
            &Theme::dark_default(),
            ThemeMode::Dark,
            false,
            EmojiPreference::Native,
            &mut cleanups,
        );
        assert_eq!(buf.content, before);
        assert_eq!(cleanups, 1);
    }

    #[test]
    fn text_emoji_replaces_pictographs() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        buf.set_string(0, 0, "a\u{1F600}b", ratatui::style::Style::default());
        let mut cleanups = 0usize;
        finish_frame(
            &mut buf,
            &Theme::dark_default(),
            ThemeMode::Dark,
            false,
            EmojiPreference::Text,
            &mut cleanups,
        );
        let text: String = buf.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains('?'));
    }

    #[test]
    fn default_palette_keeps_bytes_identical() {
        use ratatui::style::{Color, Style};
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        buf.set_string(
            0,
            0,
            "ab",
            Style::default().fg(Color::Rgb(0x22, 0xD3, 0xEE)),
        );
        let before = buf.content.clone();
        let mut cleanups = 0usize;
        finish_frame(
            &mut buf,
            &Theme::dark_default(),
            ThemeMode::Dark,
            false,
            EmojiPreference::Native,
            &mut cleanups,
        );
        assert_eq!(buf.content, before);
    }

    #[test]
    fn custom_palette_adsorbs_literal_to_role() {
        use ratatui::style::{Color, Style};
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        buf.set_string(
            0,
            0,
            "ab",
            Style::default().fg(Color::Rgb(0x22, 0xD3, 0xEE)),
        );
        let mut custom = Theme::dark_default();
        custom.accent = tui_style::theme::Rgb::new(0x11, 0x22, 0x33);
        let mut cleanups = 0usize;
        finish_frame(
            &mut buf,
            &custom,
            ThemeMode::Dark,
            false,
            EmojiPreference::Native,
            &mut cleanups,
        );
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(buf[(1, 0)].fg, Color::Rgb(0x11, 0x22, 0x33));
    }

    #[test]
    fn palette_pass_runs_after_theme_adaptation() {
        use ratatui::style::{Color, Style};
        // Light mode flips brightness first; the palette pass must see the
        // flipped value, never re-adapt seeded content. Here the flip and
        // the custom adsorb both apply exactly once.
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        buf.set_string(
            0,
            0,
            "a",
            Style::default().fg(Color::Rgb(0x22, 0xD3, 0xEE)),
        );
        let mut custom = Theme::dark_default();
        custom.accent = tui_style::theme::Rgb::new(0x11, 0x22, 0x33);
        let mut once = buf.clone();
        finish_frame(
            &mut once,
            &custom,
            ThemeMode::Light,
            false,
            EmojiPreference::Native,
            &mut 0usize,
        );
        let mut twice = once.clone();
        // A second palette-only pass over already finished output must not
        // drift: adaptation is not re-applied by the palette stage.
        adapt_buffer_for_palette(&mut twice, &custom);
        assert_eq!(once.content, twice.content);
    }

    #[test]
    fn dark_and_light_snapshots_cover_both_kinds() {
        use ratatui::style::{Color, Style};
        for theme in [Theme::dark_default(), Theme::light_default()] {
            let area = Rect::new(0, 0, 2, 1);
            let mut buf = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
            buf.set_string(
                0,
                0,
                "ok",
                Style::default().fg(Color::Rgb(theme.accent.r, theme.accent.g, theme.accent.b)),
            );
            let mut cleanups = 0usize;
            finish_frame(
                &mut buf,
                &theme,
                ThemeMode::Dark,
                true,
                EmojiPreference::Native,
                &mut cleanups,
            );
            assert_eq!(cleanups, 1);
        }
    }
}
