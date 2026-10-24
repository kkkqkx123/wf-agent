//! Panel-scoped stream text selection over logical source lines.
//!
//! A terminal's native selection spans the full terminal row, sweeping up
//! borders and neighbouring panels. The helpers here confine selection to a
//! single panel rectangle with natural stream semantics: the first line runs
//! from the click column to its end, middle lines copy whole, and the last
//! line runs up to the release column.
//!
//! Selections are stored as logical [`TextPos`] values (source line plus
//! display-column offset), never as screen cells, so the same characters
//! stay selected when the panel is resized, rewrapped or scrolled. Geometry
//! answers only map between the two spaces; the caller owns the scroll
//! offset, the panel rectangle and the hit testing against sibling panels.
//!
//! Columns count Unicode display cells, matching the transcript reflow rule
//! in [`crate::wrap_map`]; slicing reuses
//! [`crate::wrap_map::WrappedColumnMap::slice_columns`] so wide glyphs are
//! never split.

use std::collections::HashSet;

use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

/// Logical text position: a source line plus a display-column offset into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextPos {
    /// Source line index.
    pub line: usize,
    /// Display-column offset within the line.
    pub col: usize,
}

impl TextPos {
    /// Logical position for a source line and display column.
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Wrap behaviour for a selectable panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelWrapMode {
    /// Lines wider than the panel wrap onto multiple rows.
    #[default]
    Wrap,
    /// Every source line occupies exactly one row, clipped at the edge.
    Clip,
}

/// Shared geometry between point mapping and highlight computation.
///
/// Built from per-line display widths plus the effective panel width; the
/// optional continuation-marker column is excluded up front so marker cells
/// never map to a character.
#[derive(Debug, Clone)]
pub struct PanelGeometry {
    line_widths: Vec<usize>,
    wrap_width: usize,
    mode: PanelWrapMode,
    /// Exclusive prefix sums of wrapped-row counts, one entry per line.
    cumulative: Vec<usize>,
    total: usize,
}

impl PanelGeometry {
    /// Build geometry for `line_widths` (display columns per source line) at
    /// panel width `width`. When `reserve_marker` holds and wrapping applies,
    /// one column is reserved for the continuation marker.
    pub fn new(
        line_widths: Vec<usize>,
        width: usize,
        mode: PanelWrapMode,
        reserve_marker: bool,
    ) -> Self {
        let wrap_width = if reserve_marker && mode == PanelWrapMode::Wrap && width >= 2 {
            width - 1
        } else {
            width
        };
        let mut cumulative = Vec::with_capacity(line_widths.len());
        let mut total = 0usize;
        for len in &line_widths {
            cumulative.push(total);
            total = total.saturating_add(Self::rows_for_line(*len, wrap_width, mode));
        }
        Self {
            line_widths,
            wrap_width,
            mode,
            cumulative,
            total,
        }
    }

    /// Effective wrap width (marker column already excluded).
    pub fn wrap_width(&self) -> usize {
        self.wrap_width
    }

    /// Wrap behaviour this geometry was built with.
    pub fn mode(&self) -> PanelWrapMode {
        self.mode
    }

    /// Number of source lines.
    pub fn line_count(&self) -> usize {
        self.line_widths.len()
    }

    /// Total wrapped rows across all lines (at least one per line).
    pub fn total_rows(&self) -> usize {
        self.total.max(self.line_count().min(1))
    }

    /// Display width of one source line, if present.
    pub fn line_width(&self, line: usize) -> Option<usize> {
        self.line_widths.get(line).copied()
    }

    /// Map a logical position to a (wrapped row, in-row column) pair.
    /// Inputs are clamped to the last line and line end.
    pub fn textpos_to_row_col(&self, pos: TextPos) -> (usize, usize) {
        if self.line_widths.is_empty() {
            return (0, 0);
        }
        let line = pos.line.min(self.line_widths.len() - 1);
        let len = self.line_widths[line];
        let col = pos.col.min(len);
        if self.mode == PanelWrapMode::Clip || self.wrap_width == 0 {
            return (self.cumulative[line], col);
        }
        let rows = Self::rows_for_line(len, self.wrap_width, self.mode);
        let row_in_line = (col / self.wrap_width).min(rows.saturating_sub(1));
        let col_in_row = col.saturating_sub(row_in_line.saturating_mul(self.wrap_width));
        (
            self.cumulative[line].saturating_add(row_in_line),
            col_in_row,
        )
    }

    /// Map a wrapped row plus in-row column back to a logical position.
    /// Inputs are clamped to the content; `usize::MAX` as column selects
    /// the line end (used by drag auto-scroll at the viewport edge).
    pub fn row_col_to_textpos(&self, row: usize, col: usize) -> TextPos {
        if self.line_widths.is_empty() {
            return TextPos::new(0, 0);
        }
        let (line, row_in_line) = self.locate(row);
        let len = self.line_widths[line];
        let base = row_in_line.saturating_mul(self.wrap_width);
        TextPos::new(line, base.saturating_add(col).min(len))
    }

    /// Map a terminal cell to a logical position. The cell is clamped to the
    /// panel rectangle first, so clicks outside the panel land on the
    /// nearest edge. Returns `None` for empty content or an empty area.
    pub fn point_to_textpos(&self, point: (u16, u16), area: Rect, scroll: u16) -> Option<TextPos> {
        if self.line_widths.is_empty() || area.width == 0 || area.height == 0 {
            return None;
        }
        let (col, row) = point;
        let local_row = if row < area.y {
            0
        } else {
            (row.saturating_sub(area.y)).min(area.height.saturating_sub(1))
        };
        let local_col = if col < area.x {
            0
        } else {
            col.saturating_sub(area.x)
        };
        let target = usize::from(scroll).saturating_add(usize::from(local_row));
        Some(self.row_col_to_textpos(target, usize::from(local_col)))
    }

    /// Extract the selected text with stream semantics. Returns `None` when
    /// the range covers no readable characters (empty or whitespace-only).
    pub fn extract_text(
        &self,
        anchor: TextPos,
        cursor: TextPos,
        source_lines: &[String],
        exclude: Option<&HashSet<TextPos>>,
    ) -> Option<String> {
        if self.line_widths.is_empty() {
            return None;
        }
        let (start, end) = ordered(anchor, cursor);
        let mut out = Vec::new();
        for line in start.line..=end.line {
            let Some(text) = source_lines.get(line) else {
                break;
            };
            let len = self.line_widths.get(line).copied().unwrap_or(0);
            let (from, to) = self.range_for_line(line, start, end, len);
            if from >= to {
                continue;
            }
            let mut sliced = crate::wrap_map::WrappedColumnMap::slice_columns(text, from, to);
            if let Some(excluded) = exclude {
                sliced = strip_positions_in(&sliced, line, excluded);
            }
            out.push(sliced);
        }
        let joined = out.join("\n");
        (!joined.trim().is_empty()).then_some(joined)
    }

    /// Highlight cells for the visible window: absolute terminal
    /// `(row, col_from, col_to_exclusive)` triples. Only the visible lines
    /// are scanned, so huge bodies and single enormous lines stay cheap.
    pub fn highlight_cells(
        &self,
        anchor: TextPos,
        cursor: TextPos,
        area: Rect,
        scroll: u16,
    ) -> Vec<(u16, u16, u16)> {
        if self.line_widths.is_empty() || area.width == 0 || area.height == 0 {
            return Vec::new();
        }
        let (start, end) = ordered(anchor, cursor);
        let height = usize::from(area.height);
        let first_visible = self.row_col_to_textpos(usize::from(scroll), 0).line;
        let last_visible = self
            .row_col_to_textpos(
                usize::from(scroll).saturating_add(height).saturating_sub(1),
                0,
            )
            .line;
        let low = start.line.max(first_visible);
        let high = end.line.min(last_visible);
        if low > high {
            return Vec::new();
        }
        let mut cells = Vec::new();
        for line in low..=high {
            let len = self.line_widths.get(line).copied().unwrap_or(0);
            let (from, to) = self.range_for_line(line, start, end, len);
            if from >= to {
                continue;
            }
            let base_row = self.cumulative[line];
            let rows_in_line = Self::rows_for_line(len, self.wrap_width, self.mode);
            let scroll_usize = usize::from(scroll);
            let scan_start = scroll_usize.saturating_sub(base_row);
            let scan_end = scroll_usize
                .saturating_add(height)
                .saturating_sub(base_row)
                .min(rows_in_line);
            if scan_start >= scan_end {
                continue;
            }
            for wrapped in scan_start..scan_end {
                let row_start = wrapped.saturating_mul(self.wrap_width);
                let row_end = row_start.saturating_add(self.wrap_width).min(len);
                let seg_from = from.max(row_start);
                let seg_to = to.min(row_end);
                if seg_from >= seg_to {
                    continue;
                }
                let local_row = base_row
                    .saturating_add(wrapped)
                    .saturating_sub(scroll_usize);
                let row = area.y.saturating_add(local_row as u16);
                let col_from = area
                    .x
                    .saturating_add(seg_from.saturating_sub(row_start) as u16);
                let col_to = area
                    .x
                    .saturating_add(seg_to.saturating_sub(row_start) as u16);
                cells.push((row, col_from, col_to));
            }
        }
        cells
    }

    fn rows_for_line(len: usize, wrap_width: usize, mode: PanelWrapMode) -> usize {
        match mode {
            PanelWrapMode::Clip => 1,
            PanelWrapMode::Wrap => {
                if wrap_width == 0 || len == 0 {
                    1
                } else {
                    len.div_ceil(wrap_width).max(1)
                }
            }
        }
    }

    /// Wrapped-row start of one source line, if present.
    pub fn line_start_row(&self, line: usize) -> Option<usize> {
        self.cumulative.get(line).copied()
    }

    /// Locate a wrapped row: owning source line plus the row index inside
    /// that line. Rows past the end clamp to the last line.
    pub fn locate_row(&self, row: usize) -> (usize, usize) {
        self.locate(row)
    }

    fn locate(&self, row: usize) -> (usize, usize) {
        if self.cumulative.is_empty() {
            return (0, 0);
        }
        let mut low = 0usize;
        let mut high = self.cumulative.len();
        while low + 1 < high {
            let mid = low.saturating_add(high) / 2;
            if self.cumulative[mid] <= row {
                low = mid;
            } else {
                high = mid;
            }
        }
        (low, row.saturating_sub(self.cumulative[low]))
    }

    fn range_for_line(
        &self,
        line: usize,
        start: TextPos,
        end: TextPos,
        len: usize,
    ) -> (usize, usize) {
        let _ = self;
        if start.line == end.line {
            (
                start.col.min(len),
                end.col
                    .min(len)
                    .saturating_add(1)
                    .min(len)
                    .max(start.col.min(len)),
            )
        } else if line == start.line {
            (start.col.min(len), len)
        } else if line == end.line {
            (0, end.col.min(len).saturating_add(1).min(len))
        } else {
            (0, len)
        }
    }
}

/// Order two endpoints so the first is the earlier document position.
/// Backwards drags select the same text as forwards drags.
pub fn ordered(a: TextPos, b: TextPos) -> (TextPos, TextPos) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Display width of each source line in Unicode cells.
pub fn display_widths(source_lines: &[String]) -> Vec<usize> {
    source_lines
        .iter()
        .map(|line| line.as_str().width())
        .collect()
}

/// Remove characters at excluded logical positions from already-extracted
/// text (used to skip decorative characters from the copied payload).
/// `base_line` is the source line of `text`'s first row.
pub fn strip_positions(text: &str, base_line: usize, exclude: &HashSet<TextPos>) -> String {
    if exclude.is_empty() {
        return text.to_string();
    }
    let mut out_lines = Vec::new();
    for (offset, line) in text.lines().enumerate() {
        out_lines.push(strip_positions_in(line, base_line + offset, exclude));
    }
    let mut out = out_lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn strip_positions_in(text: &str, line: usize, exclude: &HashSet<TextPos>) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = String::new();
    let mut col = 0usize;
    for grapheme in text.graphemes(true) {
        let width = unicode_width::UnicodeWidthStr::width(grapheme).max(1);
        let mut skip = false;
        for offset in 0..width {
            if exclude.contains(&TextPos::new(line, col + offset)) {
                skip = true;
                break;
            }
        }
        if !skip {
            out.push_str(grapheme);
        }
        col += width;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|item| item.to_string()).collect()
    }

    fn geometry_for(source: &[String], width: usize) -> PanelGeometry {
        PanelGeometry::new(display_widths(source), width, PanelWrapMode::Wrap, false)
    }

    #[test]
    fn ordered_normalises_backwards_drags() {
        let anchor = TextPos::new(2, 4);
        let cursor = TextPos::new(0, 1);
        assert_eq!(ordered(anchor, cursor), (cursor, anchor));
        assert_eq!(ordered(cursor, anchor), (cursor, anchor));
    }

    #[test]
    fn stream_semantics_cover_first_partial_middle_whole_last_partial() {
        let source = lines(&["hello world", "middle line", "last line here"]);
        let geometry = geometry_for(&source, 40);
        let text = geometry
            .extract_text(TextPos::new(0, 6), TextPos::new(2, 3), &source, None)
            .expect("readable selection");
        assert_eq!(text, "world\nmiddle line\nlast");
    }

    #[test]
    fn single_char_click_selects_that_character() {
        let source = lines(&["hello"]);
        let geometry = geometry_for(&source, 40);
        let text = geometry
            .extract_text(TextPos::new(0, 1), TextPos::new(0, 1), &source, None)
            .expect("single char");
        assert_eq!(text, "e");
    }

    #[test]
    fn whitespace_only_selection_yields_nothing() {
        let source = lines(&["   ", "  "]);
        let geometry = geometry_for(&source, 40);
        assert_eq!(
            geometry.extract_text(TextPos::new(0, 0), TextPos::new(1, 1), &source, None),
            None
        );
        assert_eq!(
            geometry.extract_text(TextPos::new(0, 0), TextPos::new(0, 0), &[], None),
            None
        );
    }

    #[test]
    fn point_mapping_clamps_outside_cells_to_the_nearest_edge() {
        let source = lines(&["hello", "world"]);
        let geometry = geometry_for(&source, 40);
        let area = Rect::new(10, 5, 20, 4);
        assert_eq!(
            geometry.point_to_textpos((0, 0), area, 0),
            Some(TextPos::new(0, 0))
        );
        assert_eq!(
            geometry.point_to_textpos((200, 200), area, 0),
            Some(TextPos::new(1, 5))
        );
        assert_eq!(
            geometry.point_to_textpos((12, 6), area, 0),
            Some(TextPos::new(1, 2))
        );
    }

    #[test]
    fn same_logical_range_survives_rewrap() {
        let source = lines(&["hello world wrapped text", "second line"]);
        let narrow = PanelGeometry::new(display_widths(&source), 8, PanelWrapMode::Wrap, false);
        let wide = PanelGeometry::new(display_widths(&source), 40, PanelWrapMode::Wrap, false);
        let anchor = TextPos::new(0, 6);
        let cursor = TextPos::new(1, 5);
        assert_eq!(
            narrow.extract_text(anchor, cursor, &source, None),
            wide.extract_text(anchor, cursor, &source, None)
        );
    }

    #[test]
    fn clip_mode_keeps_one_row_per_line() {
        let source = lines(&["a fairly long line", "short"]);
        let geometry = PanelGeometry::new(display_widths(&source), 4, PanelWrapMode::Clip, false);
        assert_eq!(geometry.total_rows(), 2);
        assert_eq!(geometry.textpos_to_row_col(TextPos::new(0, 10)), (0, 10));
        let text = geometry
            .extract_text(TextPos::new(0, 0), TextPos::new(0, 3), &source, None)
            .expect("clip range");
        assert_eq!(text, "a fa");
    }

    #[test]
    fn highlight_cells_stay_within_the_visible_window() {
        let long: String = "x".repeat(10_000);
        let source = vec![long];
        let geometry = geometry_for(&source, 40);
        let area = Rect::new(0, 0, 40, 5);
        let cells = geometry.highlight_cells(TextPos::new(0, 0), TextPos::new(0, 9_999), area, 0);
        assert_eq!(cells.len(), 5);
        for (row, from, to) in &cells {
            assert!(*row < 5);
            assert!(*from < *to && *to <= 40);
        }
        let scrolled =
            geometry.highlight_cells(TextPos::new(0, 0), TextPos::new(0, 9_999), area, 100);
        assert_eq!(scrolled.len(), 5);
        assert_eq!(scrolled[0].0, 0);
    }

    #[test]
    fn marker_reservation_keeps_marker_cells_unmapped() {
        let source = lines(&["abcdefgh"]);
        let geometry = PanelGeometry::new(display_widths(&source), 5, PanelWrapMode::Wrap, true);
        assert_eq!(geometry.wrap_width(), 4);
        assert_eq!(geometry.total_rows(), 2);
        assert_eq!(geometry.textpos_to_row_col(TextPos::new(0, 7)), (1, 3));
    }

    #[test]
    fn excluded_positions_are_stripped_from_the_payload() {
        let mut excluded = HashSet::new();
        excluded.insert(TextPos::new(0, 1));
        let stripped = strip_positions("abc", 0, &excluded);
        assert_eq!(stripped, "ac");
        let source = lines(&["abc"]);
        let geometry = geometry_for(&source, 40);
        let text = geometry
            .extract_text(
                TextPos::new(0, 0),
                TextPos::new(0, 2),
                &source,
                Some(&excluded),
            )
            .expect("rest stays readable");
        assert_eq!(text, "ac");
    }

    #[test]
    fn empty_content_maps_to_nothing() {
        let geometry = PanelGeometry::new(Vec::new(), 40, PanelWrapMode::Wrap, false);
        let area = Rect::new(0, 0, 40, 10);
        assert_eq!(geometry.point_to_textpos((1, 1), area, 0), None);
        assert_eq!(
            geometry.extract_text(TextPos::new(0, 0), TextPos::new(0, 0), &[], None),
            None
        );
        assert!(geometry
            .highlight_cells(TextPos::new(0, 0), TextPos::new(0, 0), area, 0)
            .is_empty());
    }
}
