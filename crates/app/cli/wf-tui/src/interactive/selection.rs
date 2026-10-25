//! Logical transcript selection geometry.
//!
//! Selection state is stored in [`TextPos`] coordinates over the rendered
//! plain projection of the committed scrollback. The projection is built per
//! scrollback entry at an unbounded width, so soft wrapping never enters the
//! coordinate system: scrolling only changes the viewport mapping and a
//! resize only changes how a logical position maps back to a screen cell.
//!
//! Hit testing reads the prepared scrollback row index to find the entry
//! under a viewport row; only that entry's geometry is materialised, so the
//! cost follows the selected rows instead of the whole session.

use std::sync::Arc;

use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use tui_components::scrollbar;
use tui_core::panel_select::{ordered, Motion, PanelGeometry, PanelWrapMode, TextPos};

use super::{InteractiveController, TranscriptSelection};

impl InteractiveController {
    /// Refresh the width-independent rendered plain projection of the
    /// committed scrollback. Entries are re-rendered only when their identity
    /// changes; offsets are rebuilt whenever the entry set grows or shrinks.
    pub(crate) fn sync_transcript_layout(&mut self) {
        let count = self.scrollback.len();
        let mut changed = self.logical_rows.len() != count;
        if self.logical_rows.len() > count {
            self.logical_rows.truncate(count);
        }
        for index in 0..count {
            let identity = self.scrollback[index].identity();
            let stale = match self.logical_rows.get(index) {
                Some((cached, _)) => *cached != identity,
                None => true,
            };
            if stale {
                let rows = Arc::new(self.scrollback[index].logical_plain_lines());
                let entry = (identity, rows);
                if index < self.logical_rows.len() {
                    self.logical_rows[index] = entry;
                } else {
                    self.logical_rows.push(entry);
                }
                changed = true;
            }
        }
        if !changed && self.logical_offsets.len() == count {
            return;
        }
        self.logical_offsets.clear();
        let mut offset = 0usize;
        for (_, rows) in &self.logical_rows {
            self.logical_offsets.push(offset);
            offset = offset.saturating_add(rows.len());
        }
        self.logical_total = offset;
    }

    /// Geometry for one scrollback entry's logical rows at the current text
    /// width.
    pub(crate) fn message_geometry(&self, message: usize) -> Option<PanelGeometry> {
        let (_, rows) = self.logical_rows.get(message)?;
        let width = usize::from(self.last_layout_width.max(1));
        let widths: Vec<usize> = rows.iter().map(|row| row.as_str().width()).collect();
        Some(PanelGeometry::new(
            widths,
            width,
            PanelWrapMode::Wrap,
            false,
        ))
    }

    /// Geometry across every entry's logical rows, used by keyboard motion
    /// and whole-panel selection. Only widths are collected, never text.
    pub(crate) fn transcript_geometry(&self) -> Option<PanelGeometry> {
        if self.logical_rows.is_empty() {
            return None;
        }
        let mut widths = Vec::with_capacity(self.logical_total);
        for (_, rows) in &self.logical_rows {
            widths.extend(rows.iter().map(|row| row.as_str().width()));
        }
        Some(PanelGeometry::new(
            widths,
            usize::from(self.last_layout_width.max(1)),
            PanelWrapMode::Wrap,
            false,
        ))
    }

    /// First display row of the viewport in the flat display-row space.
    pub(super) fn viewport_top_row(&self) -> usize {
        let width = self.last_layout_width.max(1);
        let streaming_rows = self
            .streaming
            .as_ref()
            .map(|line| line.display_lines(width).len())
            .unwrap_or(0);
        let capacity = usize::from(self.last_layout_height.max(1));
        let total = self.prep.total_rows().saturating_add(streaming_rows);
        scrollbar::top_for_tail(
            total,
            capacity,
            self.view_scroll.saturating_add(self.tail_lag),
        )
    }

    /// Committed scrollback entry owning a flat display row.
    pub(crate) fn message_at_display_row(&self, target: usize) -> Option<(usize, usize)> {
        let ranges = self.prep.index();
        let upper = ranges.partition_point(|range| range.start <= target);
        if upper == 0 {
            return None;
        }
        let index = upper - 1;
        let range = ranges[index];
        if index >= self.scrollback.len() || target >= range.start.saturating_add(range.len) {
            return None;
        }
        Some((index, target.saturating_sub(range.start)))
    }

    /// Logical position for a viewport-relative row and display column.
    /// Streaming rows and rows outside the viewport return `None`.
    pub(crate) fn transcript_pos_at_display_row(
        &self,
        display_row: usize,
        col: usize,
    ) -> Option<TextPos> {
        let capacity = usize::from(self.last_layout_height.max(1));
        if capacity == 0 || display_row >= capacity {
            return None;
        }
        let target = self.viewport_top_row().saturating_add(display_row);
        if target >= self.prep.total_rows() {
            return None;
        }
        let (message, row_in_message) = self.message_at_display_row(target)?;
        let geometry = self.message_geometry(message)?;
        let last_row = geometry.total_rows().saturating_sub(1);
        let local = geometry.row_col_to_textpos(row_in_message.min(last_row), col);
        let base = self.logical_offsets.get(message).copied()?;
        Some(TextPos::new(base.saturating_add(local.line), local.col))
    }

    /// Whether transcript selection keys should take precedence over the
    /// active footer view. Only the prompt view competes for them (search
    /// mode keeps its own navigation, approval and question views keep
    /// their own keys).
    pub(crate) fn transcript_selection_keys_active(&self) -> bool {
        self.footer.view == crate::footer::FooterView::Prompt
            && !self.footer.composer.search_active()
    }

    /// Visible wrapped rows, used as one keyboard page step.
    pub(crate) fn transcript_page_rows(&self) -> u16 {
        self.last_layout_height.saturating_sub(1).max(1)
    }

    /// Select every committed rendered character.
    pub fn select_all_transcript(&mut self) -> bool {
        let Some((anchor, head)) = self.transcript_geometry().and_then(|g| g.select_all()) else {
            return false;
        };
        self.transcript_regions.clear();
        self.transcript_selection = Some(TranscriptSelection { anchor, head });
        self.ensure_transcript_head_visible(head);
        true
    }

    /// Extend the active selection by one keyboard motion. Starting from no
    /// selection anchors at the bottom visible row first.
    pub(crate) fn extend_transcript_selection(&mut self, motion: Motion) -> bool {
        let Some(area) = self.last_scroll_area else {
            return false;
        };
        let previous = match self.transcript_selection {
            Some(selection) => selection,
            None => {
                let row = usize::from(area.height.saturating_sub(1));
                let Some(pos) = self.transcript_pos_at_display_row(row, 0) else {
                    return false;
                };
                TranscriptSelection {
                    anchor: pos,
                    head: pos,
                }
            }
        };
        let Some(geometry) = self.transcript_geometry() else {
            return false;
        };
        let head = geometry.move_pos(previous.head, motion);
        if head == previous.head && self.transcript_selection.is_some() {
            return false;
        }
        self.transcript_selection = Some(TranscriptSelection {
            anchor: previous.anchor,
            head,
        });
        self.ensure_transcript_head_visible(head);
        true
    }

    /// Scroll the viewport just enough to show the logical head row.
    fn ensure_transcript_head_visible(&mut self, head: TextPos) {
        let upper = self
            .logical_offsets
            .partition_point(|base| *base <= head.line);
        if upper == 0 {
            return;
        }
        let message = upper - 1;
        let base = self.logical_offsets[message];
        let Some(geometry) = self.message_geometry(message) else {
            return;
        };
        let local = TextPos::new(head.line.saturating_sub(base), head.col);
        let (local_row, _) = geometry.textpos_to_row_col(local);
        let head_row = self
            .prep
            .source_row_start(message)
            .unwrap_or(0)
            .saturating_add(local_row);
        let capacity = usize::from(self.last_layout_height.max(1));
        let total = self.prep.total_rows();
        if capacity == 0 || total == 0 {
            return;
        }
        let max_scroll = total.saturating_sub(capacity);
        let top = scrollbar::top_for_tail(
            total,
            capacity,
            self.view_scroll.saturating_add(self.tail_lag),
        );
        if head_row < top {
            self.tail_lag = 0;
            self.view_scroll = max_scroll.saturating_sub(head_row);
            self.soft_invalidate();
        } else if head_row >= top.saturating_add(capacity) {
            let desired = head_row.saturating_add(1).saturating_sub(capacity);
            self.tail_lag = 0;
            self.view_scroll = max_scroll.saturating_sub(desired.min(max_scroll));
            self.soft_invalidate();
        }
    }

    /// Extracted text of the selection over the rendered plain projection.
    /// Entries are joined in document order; whitespace-only results yield
    /// nothing.
    pub(crate) fn selected_transcript_text(
        &self,
        selection: TranscriptSelection,
    ) -> Option<String> {
        let (start, end) = ordered(selection.anchor, selection.head);
        let mut parts = Vec::new();
        for message in 0..self.logical_rows.len() {
            let Some(base) = self.logical_offsets.get(message).copied() else {
                break;
            };
            let rows = &self.logical_rows[message].1;
            if rows.is_empty() {
                continue;
            }
            let last_line = rows.len() - 1;
            let message_end = base.saturating_add(last_line);
            if end.line < base || start.line > message_end {
                continue;
            }
            let local_start = if start.line <= base {
                TextPos::new(0, if start.line < base { 0 } else { start.col })
            } else {
                TextPos::new(start.line - base, start.col)
            };
            let local_end = if end.line > message_end {
                let width = rows[last_line].as_str().width();
                TextPos::new(last_line, width)
            } else {
                TextPos::new(end.line - base, end.col)
            };
            let Some(geometry) = self.message_geometry(message) else {
                continue;
            };
            if let Some(text) = geometry.extract_text(local_start, local_end, rows, None) {
                parts.push(text);
            }
        }
        let joined = parts.join("\n");
        (!joined.trim().is_empty()).then_some(joined)
    }

    /// Highlight cells for every selection range in absolute terminal
    /// coordinates. Only entries intersecting the visible window are scanned.
    pub(crate) fn transcript_highlight_cells(&self, area: Rect) -> Vec<(u16, u16, u16)> {
        if area.width == 0 || area.height == 0 {
            return Vec::new();
        }
        let mut cells = Vec::new();
        for selection in self
            .transcript_regions
            .iter()
            .copied()
            .chain(self.transcript_selection)
        {
            self.highlight_transcript_range(area, selection, &mut cells);
        }
        cells
    }

    /// Append one range's visible highlight cells.
    fn highlight_transcript_range(
        &self,
        area: Rect,
        selection: TranscriptSelection,
        cells: &mut Vec<(u16, u16, u16)>,
    ) {
        let (start, end) = ordered(selection.anchor, selection.head);
        let top = self.viewport_top_row();
        let capacity = usize::from(area.height);
        for message in 0..self.logical_rows.len() {
            let Some(base) = self.logical_offsets.get(message).copied() else {
                break;
            };
            let rows = &self.logical_rows[message].1;
            if rows.is_empty() {
                continue;
            }
            let message_end = base.saturating_add(rows.len() - 1);
            if end.line < base || start.line > message_end {
                continue;
            }
            let Some(range) = self.prep.index().get(message).copied() else {
                continue;
            };
            let Some(geometry) = self.message_geometry(message) else {
                continue;
            };
            let display_end = range.start.saturating_add(geometry.total_rows());
            let visible_start = range.start.max(top);
            let visible_end = display_end.min(top.saturating_add(capacity));
            if visible_start >= visible_end {
                continue;
            }
            let local_start = if start.line <= base {
                TextPos::new(0, if start.line < base { 0 } else { start.col })
            } else {
                TextPos::new(start.line - base, start.col)
            };
            let local_end = if end.line > message_end {
                TextPos::new(rows.len() - 1, rows[rows.len() - 1].as_str().width())
            } else {
                TextPos::new(end.line - base, end.col)
            };
            let local_area = Rect::new(
                area.x,
                area.y.saturating_add((visible_start - top) as u16),
                area.width,
                (visible_end - visible_start) as u16,
            );
            let scroll = (visible_start - range.start) as u16;
            cells.extend(geometry.highlight_cells(local_start, local_end, local_area, scroll));
        }
    }
}
