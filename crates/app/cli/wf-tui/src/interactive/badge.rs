//! Floating jump-to-latest badge: text variants and painting.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::interactive::mouse_hit;
use crate::interactive::InteractiveController;
use crate::theme::Theme;

impl InteractiveController {
    /// Whether the detached-history badge belongs on screen.
    pub(super) fn should_show_jump_badge(&self) -> bool {
        self.view_scroll > 0
    }

    /// Paint the floating jump-to-latest badge over the last scrollback row.
    /// When the gutter shows, the badge anchors to the text area bottom-right
    /// so it never covers the bar; otherwise it uses the full scroll area.
    /// When an anchor sits at or above the viewport top, its kind is
    /// appended so detached readers see the nearest jump target. Returns the
    /// hit rectangle when painted so the event layer can match clicks.
    pub(super) fn paint_jump_badge(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
    ) -> Option<Rect> {
        if !self.should_show_jump_badge() || area.width == 0 || area.height == 0 {
            return None;
        }
        let badge_area = mouse_hit::badge_area_for(area, self.scrollbar_currently_visible(area));
        let hidden = self.view_scroll;
        let anchor_hint = self.anchor_above_viewport().map(|entry| entry.kind.label());
        let full = jump_badge_text_with_anchor(hidden, anchor_hint);
        let short = jump_badge_short_text(hidden);
        let rect = mouse_hit::badge_rect(badge_area, &full, &short)?;
        let text = pick_jump_badge_text(&full, &short, badge_area.width)?;
        use ratatui::style::Modifier;
        use tui_style::theme::ColorRole;
        let style = theme
            .style_for_role(ColorRole::Accent)
            .add_modifier(Modifier::BOLD);
        frame.buffer_mut().set_string(rect.x, rect.y, text, style);
        Some(rect)
    }
}

/// Full badge text carrying the hidden row count and the key hint.
pub(crate) fn jump_badge_text(hidden: usize) -> String {
    format!("↓ {hidden} more · End for latest")
}

/// Full badge text with the nearest anchor kind above the viewport.
pub(crate) fn jump_badge_text_with_anchor(hidden: usize, anchor: Option<&str>) -> String {
    match anchor {
        Some(kind) => format!("↓ {hidden} more · {kind} above · End for latest"),
        None => jump_badge_text(hidden),
    }
}

/// Compact fallback when the viewport is too narrow for the full hint.
pub(crate) fn jump_badge_short_text(hidden: usize) -> String {
    format!("↓ {hidden} more")
}

/// Choose the full or compact badge text for the available width.
pub(crate) fn pick_jump_badge_text<'a>(
    full: &'a str,
    short: &'a str,
    width: u16,
) -> Option<&'a str> {
    use unicode_width::UnicodeWidthStr;
    if (full.width() as u16) < width {
        Some(full)
    } else if (short.width() as u16) < width {
        Some(short)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        jump_badge_short_text, jump_badge_text, jump_badge_text_with_anchor, pick_jump_badge_text,
    };

    #[test]
    fn badge_text_carries_hidden_count_and_hint() {
        assert_eq!(jump_badge_text(3), "↓ 3 more · End for latest");
        assert_eq!(jump_badge_short_text(3), "↓ 3 more");
        assert_eq!(
            jump_badge_text_with_anchor(3, Some("todo")),
            "↓ 3 more · todo above · End for latest"
        );
        assert_eq!(jump_badge_text_with_anchor(3, None), jump_badge_text(3));
    }

    #[test]
    fn badge_text_picks_compact_or_hides_when_narrow() {
        let full = jump_badge_text(12);
        let short = jump_badge_short_text(12);
        assert_eq!(pick_jump_badge_text(&full, &short, 80), Some(full.as_str()));
        assert_eq!(
            pick_jump_badge_text(&full, &short, short.len() as u16 + 1),
            Some(short.as_str())
        );
        assert_eq!(pick_jump_badge_text(&full, &short, 3), None);
    }
}
