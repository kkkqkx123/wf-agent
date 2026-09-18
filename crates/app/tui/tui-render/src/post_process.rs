//! Fixed-order frame finishing pipeline.
//!
//! Every composed frame passes these stages exactly once, so widgets and
//! overlays never need to know the active theme: brightness adaptation runs
//! first, the user palette remap runs second, emoji preference runs third,
//! and the image cleanup counter runs last even when no image widget drew.

use ratatui::buffer::Buffer;

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
    mode: ThemeMode,
    explicit: bool,
    emoji: EmojiPreference,
    cleanups: &mut usize,
) {
    tui_style::theme_mode::adapt_buffer_for_theme(buf, mode, explicit);
    adapt_buffer_for_palette(buf);
    adapt_buffer_for_emoji(buf, emoji);
    *cleanups = cleanups.saturating_add(1);
}

/// User-palette pass: snap configured literals toward the active palette.
/// Unconfigured builds keep the buffer byte-identical by design. Overrides
/// apply at theme resolution ([`tui_style::theme::resolve_theme`]), so this
/// pass is an explicit identity: component styles already carry the active
/// palette and must not be remapped twice. Use
/// [`tui_style::theme::remap_literal`] for literal-to-role mapping, never a
/// shadow helper.
pub fn adapt_buffer_for_palette(_buf: &mut Buffer) {}

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
            ThemeMode::Dark,
            false,
            EmojiPreference::Text,
            &mut cleanups,
        );
        let text: String = buf.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains('?'));
    }
}
