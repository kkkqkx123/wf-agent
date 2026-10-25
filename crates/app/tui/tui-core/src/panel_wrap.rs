//! Windowed wrap cache for selectable panels.
//!
//! [`PanelWrapCache`] owns one panel's plain text and answers the two
//! questions every frame asks: which rows are visible, and which logical
//! character each screen cell maps to. Only the visible rows are ever
//! materialised, so multi-megabyte bodies and single enormous lines stay
//! responsive. The logical mapping delegates to
//! [`crate::panel_select::PanelGeometry`]; this cache adds content
//! ownership, cheap per-frame refresh by pointer identity, and windowed row
//! slicing that never holds a whole wrapped giant in memory.

use std::sync::Arc;

use unicode_width::UnicodeWidthStr;

use crate::panel_select::{display_widths, PanelGeometry, PanelWrapMode, TextPos};

/// Owned wrap cache for one selectable panel.
#[derive(Debug, Clone, Default)]
pub struct PanelWrapCache {
    raw: Option<Arc<str>>,
    lines: Vec<String>,
    widths: Vec<usize>,
    width: usize,
    mode: PanelWrapMode,
    reserve_marker: bool,
    geometry: Option<PanelGeometry>,
}

impl PanelWrapCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh the cached text. The layout is rebuilt only when the text
    /// identity, the panel width, the wrap mode or the marker reservation
    /// changed, so calling this every frame stays cheap.
    pub fn set_content(
        &mut self,
        text: Arc<str>,
        width: usize,
        mode: PanelWrapMode,
        reserve_marker: bool,
    ) {
        let stale = match &self.raw {
            None => true,
            Some(current) => {
                !Arc::ptr_eq(current, &text)
                    || self.width != width
                    || self.mode != mode
                    || self.reserve_marker != reserve_marker
            }
        };
        if !stale {
            return;
        }
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        let widths = display_widths(&lines);
        self.geometry = Some(PanelGeometry::new(
            widths.clone(),
            width,
            mode,
            reserve_marker,
        ));
        self.raw = Some(text);
        self.lines = lines;
        self.widths = widths;
        self.width = width;
        self.mode = mode;
        self.reserve_marker = reserve_marker;
    }

    /// Whether any content is cached.
    pub fn has_content(&self) -> bool {
        self.geometry.is_some()
    }

    /// Total wrapped rows across all lines.
    pub fn total_rows(&self) -> usize {
        self.geometry.as_ref().map_or(0, PanelGeometry::total_rows)
    }

    /// Effective wrap width (marker column already excluded).
    pub fn wrap_width(&self) -> usize {
        self.geometry.as_ref().map_or(0, PanelGeometry::wrap_width)
    }

    /// Geometry built by the last content refresh.
    pub fn geometry(&self) -> Option<&PanelGeometry> {
        self.geometry.as_ref()
    }

    /// Wrap behaviour this cache was built with.
    pub fn mode(&self) -> PanelWrapMode {
        self.mode
    }

    /// Visible plain-text rows for a viewport: `height` rows from `scroll`.
    /// Huge lines contribute only their visible slice; rows are plain
    /// strings because styling stays with the render layer.
    pub fn visible_window(&self, scroll: u16, height: u16) -> Vec<String> {
        let Some(geometry) = &self.geometry else {
            return Vec::new();
        };
        if height == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut row = usize::from(scroll);
        let end = row.saturating_add(usize::from(height));
        let total = geometry.total_rows();
        while row < end && row < total {
            let (line, row_in_line) = geometry.locate_row(row);
            if line >= self.lines.len() {
                break;
            }
            let text = &self.lines[line];
            let width = self.widths.get(line).copied().unwrap_or(0);
            match self.mode {
                PanelWrapMode::Clip => {
                    out.push(clip_row(text, geometry.wrap_width()));
                    row += 1;
                }
                PanelWrapMode::Wrap => {
                    let wrap_width = geometry.wrap_width();
                    let rows_in_line = row_count(width, wrap_width);
                    let remaining = rows_in_line.saturating_sub(row_in_line);
                    let budget = end.saturating_sub(row).min(remaining);
                    out.extend(window_slices(text, wrap_width, row_in_line, budget));
                    row += budget.max(1);
                }
            }
        }
        out
    }

    /// Source line text, if cached.
    pub fn line_text(&self, line: usize) -> Option<&str> {
        self.lines.get(line).map(String::as_str)
    }

    /// Number of cached source lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Logical position for a terminal cell inside `area`.
    pub fn point_to_textpos(
        &self,
        point: (u16, u16),
        area: ratatui::layout::Rect,
        scroll: u16,
    ) -> Option<TextPos> {
        self.geometry
            .as_ref()?
            .point_to_textpos(point, area, scroll)
    }

    /// Extract the selected range as plain text.
    pub fn extract_text(
        &self,
        anchor: TextPos,
        cursor: TextPos,
        exclude: Option<&std::collections::HashSet<TextPos>>,
    ) -> Option<String> {
        self.geometry
            .as_ref()?
            .extract_text(anchor, cursor, &self.lines, exclude)
    }

    /// Highlight cells for the visible window in absolute terminal coords.
    pub fn highlight_cells(
        &self,
        anchor: TextPos,
        cursor: TextPos,
        area: ratatui::layout::Rect,
        scroll: u16,
    ) -> Vec<(u16, u16, u16)> {
        self.geometry.as_ref().map_or(Vec::new(), |geometry| {
            geometry.highlight_cells(anchor, cursor, area, scroll)
        })
    }
}

fn row_count(line_width: usize, wrap_width: usize) -> usize {
    if wrap_width == 0 || line_width == 0 {
        1
    } else {
        line_width.div_ceil(wrap_width).max(1)
    }
}

fn clip_row(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let glyph_width = grapheme.width().max(1);
        if col + glyph_width > width {
            break;
        }
        out.push_str(grapheme);
        col += glyph_width;
    }
    out
}

/// Slice `count` wrapped rows starting at `skip` without materialising the
/// whole wrapped line. Columns are display cells; wide glyphs are never
/// split and overflow glyphs move to the next row.
fn window_slices(text: &str, width: usize, skip: usize, count: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut rows: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut row_index = 0usize;
    let mut started = false;
    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let glyph_width = grapheme.width().max(1);
        let glyph_width = glyph_width.min(width);
        if current_width + glyph_width > width {
            if row_index >= skip {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
                if rows.len() >= count {
                    return rows;
                }
            } else {
                current.clear();
                current_width = 0;
            }
            row_index += 1;
            if row_index >= skip + count {
                return rows;
            }
        }
        current.push_str(grapheme);
        current_width += glyph_width;
        started = true;
    }
    if !started && skip == 0 && count > 0 {
        return vec![String::new()];
    }
    if row_index >= skip && row_index < skip + count {
        rows.push(current);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_for(text: &str, width: usize, mode: PanelWrapMode) -> PanelWrapCache {
        let mut cache = PanelWrapCache::new();
        cache.set_content(Arc::from(text), width, mode, false);
        cache
    }

    #[test]
    fn wrap_window_covers_visible_rows_only() {
        let cache = cache_for(
            "alpha beta gamma delta\nepsilon zeta",
            8,
            PanelWrapMode::Wrap,
        );
        assert!(cache.has_content());
        let first = cache.visible_window(0, 2);
        assert_eq!(first.len(), 2);
        let second = cache.visible_window(2, 2);
        assert_eq!(second.len(), 2);
        assert_ne!(first, second);
        assert_eq!(
            cache.total_rows(),
            first.len() + cache.visible_window(2, 100).len()
        );
    }

    #[test]
    fn clip_mode_keeps_one_row_per_line() {
        let cache = cache_for("abcdefgh\nxy", 4, PanelWrapMode::Clip);
        assert_eq!(cache.total_rows(), 2);
        assert_eq!(cache.visible_window(0, 10), vec!["abcd", "xy"]);
    }

    #[test]
    fn marker_reservation_narrows_the_effective_width() {
        let mut cache = PanelWrapCache::new();
        cache.set_content(Arc::from("abcdefgh"), 5, PanelWrapMode::Wrap, true);
        assert_eq!(cache.wrap_width(), 4);
        assert_eq!(cache.total_rows(), 2);
    }

    #[test]
    fn same_pointer_skips_rebuild() {
        let mut cache = PanelWrapCache::new();
        let text: Arc<str> = Arc::from("hello world");
        cache.set_content(Arc::clone(&text), 4, PanelWrapMode::Wrap, false);
        let before = cache.total_rows();
        cache.set_content(text, 4, PanelWrapMode::Wrap, false);
        assert_eq!(cache.total_rows(), before);
    }

    #[test]
    fn huge_single_line_stays_windowed() {
        let long: String = "ab ".repeat(20_000);
        let cache = cache_for(&long, 40, PanelWrapMode::Wrap);
        let window = cache.visible_window(100, 5);
        assert_eq!(window.len(), 5);
        for row in window {
            assert!(row.width() <= 40);
        }
        let scrolled = cache.visible_window(100, 5);
        assert_eq!(cache.visible_window(100, 5), scrolled);
    }

    #[test]
    fn extract_round_trips_through_the_cache() {
        let cache = cache_for("hello world", 40, PanelWrapMode::Wrap);
        let text = cache
            .extract_text(TextPos::new(0, 0), TextPos::new(0, 4), None)
            .expect("range");
        assert_eq!(text, "hello");
    }
}
