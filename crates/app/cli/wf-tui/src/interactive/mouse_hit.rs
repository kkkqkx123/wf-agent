//! Mouse hit testing for the interactive session.
//!
//! Pure geometry for the jump badge and later content hits. Rendering owns
//! the text choice; this module only turns an area plus the chosen texts
//! into a hit rectangle so paint and hit stay on one formula.

use ratatui::layout::Rect;

/// Text area for the badge: when the gutter shows the badge anchors to the
/// text area so it never covers the bar.
pub(crate) fn badge_area_for(scroll_area: Rect, scrollbar_visible: bool) -> Rect {
    if scrollbar_visible && scroll_area.width > 1 {
        Rect::new(
            scroll_area.x,
            scroll_area.y,
            scroll_area.width - 1,
            scroll_area.height,
        )
    } else {
        scroll_area
    }
}

/// Hit rectangle for the badge texts. Returns `None` when the badge does
/// not belong on screen (empty area, no text fits).
pub(crate) fn badge_rect(badge_area: Rect, full: &str, short: &str) -> Option<Rect> {
    use unicode_width::UnicodeWidthStr;
    if badge_area.width == 0 || badge_area.height == 0 {
        return None;
    }
    let text = if (full.width() as u16) < badge_area.width {
        full
    } else if (short.width() as u16) < badge_area.width {
        short
    } else {
        return None;
    };
    let text_width = text.width() as u16;
    if text_width >= badge_area.width {
        return None;
    }
    let x = badge_area.x.saturating_add(
        badge_area
            .width
            .saturating_sub(text_width)
            .saturating_sub(1),
    );
    let y = badge_area
        .y
        .saturating_add(badge_area.height.saturating_sub(1));
    Some(Rect::new(x, y, text_width, 1))
}

/// Whether a terminal cell hits the badge rectangle.
pub(crate) fn badge_contains(rect: &Rect, col: u16, row: u16) -> bool {
    row == rect.y && col >= rect.x && col < rect.x.saturating_add(rect.width)
}

/// Slice a display row text by character columns `[start, end)`.
/// Columns count characters, matching `word_at_col`.
#[cfg(test)]
pub(crate) fn slice_row_chars(text: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    chars[start..end].iter().collect()
}

/// Whether a terminal cell hits the composer input area (the bottom prompt
/// viewport). The two-column `"> "` prefix counts as inside: clicks there
/// land at the row start.
pub(crate) fn composer_contains(input_area: &Rect, col: u16, row: u16) -> bool {
    input_area.width > 2
        && input_area.height > 0
        && col >= input_area.x
        && col < input_area.x.saturating_add(input_area.width)
        && row >= input_area.y
        && row < input_area.y.saturating_add(input_area.height)
}

/// Whether a press and release belong to one click: same cell and same
/// snapshot generation. Cross-generation pairs are drags, never clicks.
pub(crate) fn press_matches_release(
    press_col: u16,
    press_row: u16,
    press_gen: u64,
    release_col: u16,
    release_row: u16,
    release_gen: u64,
) -> bool {
    press_col == release_col && press_row == release_row && press_gen == release_gen
}

/// Click target inside the scrollback content area. Badge and gutter are
/// handled before this; only summary and sub-agent start rows land here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClickTarget {
    Tool { tool_call_id: String },
    Subagent { seq: u64 },
}

/// Auto-scroll direction for a drag cell vertically outside the scroll
/// viewport: `-1` above the top edge, `+1` below the bottom edge, `None`
/// inside. The caller ticks the viewport one notch and pins the selection
/// head to the newly exposed edge row.
pub(crate) fn autoscroll_dir_for_row(row: u16, area: &Rect) -> Option<i8> {
    if area.height == 0 {
        return None;
    }
    if row < area.y {
        Some(-1)
    } else if row >= area.y.saturating_add(area.height) {
        Some(1)
    } else {
        None
    }
}

/// Word at a display column: whitespace-delimited token containing the
/// column (columns count characters, best effort for wide glyphs).
pub(crate) fn word_at_col(text: &str, col: usize) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }
        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        let end = index;
        if col >= start && col < end {
            return Some(chars[start..end].iter().collect());
        }
    }
    None
}

use super::anchors::AnchorKind;
use super::InteractiveController;

impl InteractiveController {
    /// Sequence number and scrollback position for a viewport-relative row.
    /// Returns `None` for streaming tail rows and outside rows. Uses the
    /// prepared scrollback row index, so no reflow runs here.
    fn seq_at_display_row(&self, display_row: usize) -> Option<(u64, usize)> {
        let capacity = usize::from(self.last_layout_height.max(1));
        if display_row >= capacity {
            return None;
        }
        let target = self.viewport_top_row().saturating_add(display_row);
        if target >= self.prep.total_rows() {
            return None;
        }
        let (index, _) = self.message_at_display_row(target)?;
        let seq = self.scrollback.get(index)?.seq_no();
        (seq != 0).then_some((seq, index))
    }

    /// Tool hit for a sequence number: an expandable summary row only.
    fn tool_hit_for_seq(&self, seq: u64) -> Option<String> {
        self.tool_blocks.iter().find_map(|(id, block)| {
            if block.summary_seq == seq && block.expandable {
                Some(id.clone())
            } else {
                None
            }
        })
    }

    /// Sub-agent hit for a sequence number: a live start row of an enabled
    /// kind only.
    fn sub_hit_for_seq(&self, seq: u64) -> Option<u64> {
        if self.anchor_disabled.contains(&AnchorKind::SubAgent) {
            return None;
        }
        let live = self.scrollback.iter().any(|row| row.seq_no() == seq);
        if !live {
            return None;
        }
        self.subagent_blocks
            .values()
            .find(|block| block.start_seq == seq)
            .map(|block| block.start_seq)
    }

    /// Content click target for a viewport-relative row. Sub-agent starts
    /// win over tool summaries so one row never means two actions.
    pub(crate) fn content_click_target(&self, display_row: usize) -> Option<ClickTarget> {
        let (seq, _) = self.seq_at_display_row(display_row)?;
        if let Some(seq) = self.sub_hit_for_seq(seq) {
            return Some(ClickTarget::Subagent { seq });
        }
        if let Some(tool_call_id) = self.tool_hit_for_seq(seq) {
            return Some(ClickTarget::Tool { tool_call_id });
        }
        None
    }

    /// Handle one content click: toggle the tool block or jump to the sub
    /// start. Returns true when an action landed and a full frame is due.
    pub(crate) fn handle_content_click(&mut self, display_row: usize) -> bool {
        let Some(target) = self.content_click_target(display_row) else {
            return false;
        };
        match target {
            ClickTarget::Tool { tool_call_id } => {
                self.toggle_tool_block_to(&tool_call_id, None);
                true
            }
            ClickTarget::Subagent { seq } => {
                self.jump_to_seq(seq);
                true
            }
        }
    }

    /// Plain text of one visible display row for word selection. Reads the
    /// prepared scrollback rows directly. Returns `None` for streaming rows
    /// and outside rows.
    pub(crate) fn plain_text_for_display_row(&self, display_row: usize) -> Option<String> {
        let capacity = usize::from(self.last_layout_height.max(1));
        if display_row >= capacity {
            return None;
        }
        let target = self.viewport_top_row().saturating_add(display_row);
        if target >= self.prep.total_rows() {
            return None;
        }
        let row = self.prep.rows().get(target)?;
        Some(
            row.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
        )
    }

    /// Full logical source text behind one visible display row for line
    /// selection. Returns `None` for streaming rows and outside rows.
    pub(crate) fn source_text_for_display_row(&self, display_row: usize) -> Option<String> {
        let capacity = usize::from(self.last_layout_height.max(1));
        if display_row >= capacity {
            return None;
        }
        let target = self.viewport_top_row().saturating_add(display_row);
        if target >= self.prep.total_rows() {
            return None;
        }
        let (index, _) = self.message_at_display_row(target)?;
        let text = self.scrollback.get(index)?.source_text();
        (!text.trim().is_empty()).then_some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_area_reserves_gutter_when_bar_visible() {
        let area = Rect::new(0, 0, 30, 10);
        let with_bar = badge_area_for(area, true);
        assert_eq!(with_bar.width, 29);
        assert_eq!(with_bar.x + with_bar.width, area.x + area.width - 1);
        let without = badge_area_for(area, false);
        assert_eq!(without, area);
    }

    #[test]
    fn badge_rect_anchors_bottom_right() {
        let area = Rect::new(0, 0, 30, 10);
        let rect = badge_rect(area, "↓ 3 more · End for latest", "↓ 3 more").expect("fits");
        assert_eq!(rect.height, 1);
        assert_eq!(rect.y, 9);
        assert!(rect.x + rect.width < area.x + area.width);
    }

    #[test]
    fn badge_rect_hides_when_narrow() {
        let area = Rect::new(0, 0, 3, 10);
        assert!(badge_rect(area, "↓ 3 more · End for latest", "↓ 3 more").is_none());
        let empty = Rect::new(0, 0, 0, 10);
        assert!(badge_rect(empty, "full", "short").is_none());
    }

    #[test]
    fn badge_contains_checks_single_row_span() {
        let rect = Rect::new(10, 5, 6, 1);
        assert!(badge_contains(&rect, 10, 5));
        assert!(badge_contains(&rect, 15, 5));
        assert!(!badge_contains(&rect, 16, 5));
        assert!(!badge_contains(&rect, 9, 5));
        assert!(!badge_contains(&rect, 12, 4));
        assert!(!badge_contains(&rect, 12, 6));
    }

    #[test]
    fn composer_contains_checks_input_viewport() {
        let area = Rect::new(0, 20, 40, 3);
        assert!(composer_contains(&area, 0, 20));
        assert!(composer_contains(&area, 39, 22));
        assert!(!composer_contains(&area, 40, 20));
        assert!(!composer_contains(&area, 5, 19));
        assert!(!composer_contains(&area, 5, 23));
        let narrow = Rect::new(0, 20, 2, 3);
        assert!(!composer_contains(&narrow, 0, 20));
        let flat = Rect::new(0, 20, 40, 0);
        assert!(!composer_contains(&flat, 5, 20));
    }

    #[test]
    fn row_slicing_covers_edges_and_empty() {
        assert_eq!(slice_row_chars("hello world", 0, 5), "hello");
        assert_eq!(slice_row_chars("hello world", 6, 11), "world");
        assert_eq!(slice_row_chars("hello", 3, 99), "lo");
        assert_eq!(slice_row_chars("hello", 5, 5), "");
        assert_eq!(slice_row_chars("", 0, 3), "");
    }

    #[test]
    fn press_release_requires_same_cell_and_generation() {
        assert!(press_matches_release(5, 5, 7, 5, 5, 7));
        assert!(!press_matches_release(5, 5, 7, 6, 5, 7));
        assert!(!press_matches_release(5, 5, 7, 5, 6, 7));
        assert!(!press_matches_release(5, 5, 7, 5, 5, 8));
    }

    #[test]
    fn word_selection_picks_token_at_column() {
        assert_eq!(word_at_col("hello world", 1), Some("hello".to_string()));
        assert_eq!(word_at_col("hello world", 6), Some("world".to_string()));
        assert_eq!(word_at_col("hello world", 5), None);
        assert_eq!(word_at_col("   ", 1), None);
        assert_eq!(word_at_col("", 0), None);
    }

    #[test]
    fn autoscroll_direction_follows_vertical_overflow() {
        let area = Rect::new(0, 5, 40, 10);
        assert_eq!(autoscroll_dir_for_row(4, &area), Some(-1));
        assert_eq!(autoscroll_dir_for_row(5, &area), None);
        assert_eq!(autoscroll_dir_for_row(14, &area), None);
        assert_eq!(autoscroll_dir_for_row(15, &area), Some(1));
        assert_eq!(autoscroll_dir_for_row(99, &area), Some(1));
        let flat = Rect::new(0, 5, 40, 0);
        assert_eq!(autoscroll_dir_for_row(4, &flat), None);
    }
}
