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
    let x = badge_area
        .x
        .saturating_add(badge_area.width.saturating_sub(text_width).saturating_sub(1));
    let y = badge_area
        .y
        .saturating_add(badge_area.height.saturating_sub(1));
    Some(Rect::new(x, y, text_width, 1))
}

/// Whether a terminal cell hits the badge rectangle.
pub(crate) fn badge_contains(rect: &Rect, col: u16, row: u16) -> bool {
    row == rect.y && col >= rect.x && col < rect.x.saturating_add(rect.width)
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
use tui_components::scrollbar;

impl InteractiveController {
    /// Sequence number and scrollback position for a viewport-relative row.
    /// Returns `None` for streaming tail rows and outside rows.
    fn seq_at_display_row(&self, display_row: usize) -> Option<(u64, usize)> {
        let width = self.last_layout_width.max(1);
        let capacity = usize::from(self.last_layout_height.max(1));
        if capacity == 0 {
            return None;
        }
        let mut cached_total = 0usize;
        let mut heights: Vec<usize> = Vec::with_capacity(self.scrollback.len());
        for line in &self.scrollback {
            let height = line.display_lines(width).len().max(1);
            heights.push(height);
            cached_total = cached_total.saturating_add(height);
        }
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|line| line.display_lines(width).len())
            .unwrap_or(0);
        let total = cached_total.saturating_add(streaming_len);
        if total == 0 {
            return None;
        }
        let start = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        let target = start.saturating_add(display_row);
        if target >= total || display_row >= capacity {
            return None;
        }
        if target >= cached_total {
            return None;
        }
        let mut consumed = 0usize;
        for (index, line) in self.scrollback.iter().enumerate() {
            let height = heights[index];
            if target < consumed.saturating_add(height) {
                let seq = line.seq_no();
                if seq == 0 {
                    return None;
                }
                return Some((seq, index));
            }
            consumed = consumed.saturating_add(height);
        }
        None
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

    /// Plain text of one visible display row for word selection. Returns
    /// `None` for streaming rows and outside rows.
    pub(crate) fn plain_text_for_display_row(&self, display_row: usize) -> Option<String> {
        let width = self.last_layout_width.max(1);
        let capacity = usize::from(self.last_layout_height.max(1));
        if capacity == 0 {
            return None;
        }
        let mut cached: Vec<Vec<String>> = Vec::with_capacity(self.scrollback.len());
        let mut cached_total = 0usize;
        for line in &self.scrollback {
            let rows = line.display_lines(width);
            cached_total = cached_total.saturating_add(rows.len().max(1));
            cached.push(
                rows.iter()
                    .map(|row| {
                        row.spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>()
                    })
                    .collect(),
            );
        }
        let streaming_rows: Vec<String> = self
            .streaming
            .as_ref()
            .map(|line| {
                line.display_lines(width)
                    .iter()
                    .map(|row| {
                        row.spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = cached_total.saturating_add(streaming_rows.len());
        let start = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        let target = start.saturating_add(display_row);
        if target >= total || display_row >= capacity || target >= cached_total {
            return None;
        }
        let mut consumed = 0usize;
        for rows in cached {
            if target < consumed.saturating_add(rows.len()) {
                return rows.get(target.saturating_sub(consumed)).cloned();
            }
            consumed = consumed.saturating_add(rows.len());
        }
        None
    }

    /// Full logical source text behind one visible display row for line
    /// selection. Returns `None` for streaming rows and outside rows.
    pub(crate) fn source_text_for_display_row(&self, display_row: usize) -> Option<String> {
        let (seq, _) = self.seq_at_display_row(display_row)?;
        self.scrollback
            .iter()
            .find(|row| row.seq_no() == seq)
            .map(|row| row.source_text())
            .filter(|text| !text.trim().is_empty())
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
}
