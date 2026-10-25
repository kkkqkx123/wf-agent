//! Reusable stateful selection for one panel.
//!
//! [`SelectablePanel`] combines the windowed wrap cache with a logical range
//! selection and optional frozen regions. Content arrives either as plain
//! text or as styled rows; the geometry always works on the plain projection
//! of those rows, so markdown, ANSI and syntax-highlighted content map to the
//! characters the user sees while rendering stays with the caller.
//!
//! The panel never owns scrolling: [`begin`](SelectablePanel::begin) and
//! [`extend`](SelectablePanel::extend) receive the current viewport offset and
//! rectangle, so a resize, scroll or tail-follow keeps the same characters
//! selected.

use std::collections::HashSet;
use std::sync::Arc;

use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::panel_select::{Motion, PanelGeometry, PanelWrapMode, TextPos};
use crate::panel_wrap::PanelWrapCache;

/// One logical region: anchor is the press cell, cursor the latest cell.
pub type PanelRegion = (TextPos, TextPos);

/// Selection state, content cache and geometry for one panel.
#[derive(Debug, Clone, Default)]
pub struct SelectablePanel {
    wrap: PanelWrapCache,
    /// Styled rows of the last `set_styled` call (empty for plain content).
    styled: Vec<Line<'static>>,
    /// Width the content was last synced at.
    width: usize,
    mode: PanelWrapMode,
    /// Active drag region.
    active: Option<PanelRegion>,
    /// Committed regions appended with an explicit add action.
    frozen: Vec<PanelRegion>,
    /// Logical positions excluded from copied payloads.
    exclude: HashSet<TextPos>,
    /// Whether the last content sync came from styled rows.
    styled_source: bool,
    /// Reserve one column for a continuation marker in the layout.
    marker: bool,
    /// Marker flag used by the last content sync.
    marker_applied: bool,
}

impl SelectablePanel {
    /// Empty panel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the content with plain text. Selection is kept: it is stored
    /// in logical coordinates, so a rewrap at a new width keeps the same
    /// characters selected. Callers replacing the whole body should call
    /// [`Self::clear`] first.
    pub fn set_plain(&mut self, text: Arc<str>, width: usize) {
        self.styled.clear();
        self.styled_source = false;
        self.wrap.set_content(text, width, self.mode, self.marker);
        self.width = width;
        self.marker_applied = self.marker;
    }

    /// Replace the content with styled rows. The plain projection drives
    /// geometry; the caller keeps painting `rows` if it wants the styles.
    /// Existing selection is kept for the same reasons as [`Self::set_plain`].
    pub fn set_styled(&mut self, rows: &[Line<'static>], width: usize) {
        let unchanged = self.styled_source
            && self.width == width
            && self.marker_applied == self.marker
            && self.styled.len() == rows.len()
            && self.styled == rows;
        if unchanged {
            return;
        }
        let plain = rows
            .iter()
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.styled.clear();
        self.styled.extend_from_slice(rows);
        self.styled_source = true;
        self.width = width;
        self.marker_applied = self.marker;
        self.wrap
            .set_content(Arc::from(plain), width, self.mode, self.marker);
    }

    /// Reserve one column for a continuation marker. The host paints the
    /// marker glyph; the extra column keeps its cells out of the logical
    /// geometry so marker characters are never selectable. Off by default,
    /// and the next content sync applies the change.
    pub fn set_marker(&mut self, marker: bool) {
        self.marker = marker;
    }

    /// Whether a continuation-marker column is reserved.
    pub fn marker(&self) -> bool {
        self.marker
    }

    /// Wrap behaviour used for geometry. Changing it invalidates the layout.
    pub fn set_mode(&mut self, mode: PanelWrapMode) {
        if mode != self.mode {
            self.mode = mode;
            self.wrap = PanelWrapCache::new();
            self.active = None;
            self.frozen.clear();
        }
    }

    /// Excluded logical positions stripped from copied payloads.
    pub fn set_exclude(&mut self, exclude: HashSet<TextPos>) {
        self.exclude = exclude;
    }

    /// Drop content and every selection.
    pub fn clear(&mut self) {
        self.wrap = PanelWrapCache::new();
        self.styled.clear();
        self.active = None;
        self.frozen.clear();
    }

    /// Whether any content is cached.
    pub fn has_content(&self) -> bool {
        self.wrap.has_content()
    }

    /// Styled rows captured by [`Self::set_styled`].
    pub fn styled_rows(&self) -> &[Line<'static>] {
        &self.styled
    }

    /// Geometry built by the last content sync.
    pub fn geometry(&self) -> Option<&PanelGeometry> {
        self.wrap.geometry()
    }

    /// Total wrapped rows of the cached content.
    pub fn total_rows(&self) -> usize {
        self.wrap.total_rows()
    }

    /// Active selection region, if any.
    pub fn active(&self) -> Option<PanelRegion> {
        self.active
    }

    /// Active selection cursor.
    pub fn head(&self) -> Option<TextPos> {
        self.active.map(|(_, head)| head)
    }

    /// Whether any committed region exists.
    pub fn has_regions(&self) -> bool {
        !self.frozen.is_empty()
    }

    /// Start a fresh active region at a terminal cell. Returns whether the
    /// cell mapped to content.
    pub fn begin(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool {
        self.frozen.clear();
        self.begin_active(point, area, scroll)
    }

    /// Start an active region while keeping existing ones (explicit add).
    pub fn begin_region(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool {
        if let Some((anchor, head)) = self.active.take() {
            if anchor != head {
                self.frozen.push((anchor, head));
            }
        }
        self.begin_active(point, area, scroll)
    }

    fn begin_active(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool {
        let Some(pos) = self.wrap.point_to_textpos(point, area, scroll) else {
            self.active = None;
            return false;
        };
        self.active = Some((pos, pos));
        true
    }

    /// Update the active cursor to a terminal cell. Returns whether the
    /// selection changed.
    pub fn extend(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool {
        let Some(pos) = self.wrap.point_to_textpos(point, area, scroll) else {
            return false;
        };
        match self.active {
            Some((anchor, head)) if head != pos => {
                self.active = Some((anchor, pos));
                true
            }
            Some(_) => false,
            None => false,
        }
    }

    /// Apply one keyboard motion to the active cursor.
    pub fn extend_motion(&mut self, motion: Motion) -> bool {
        let Some(geometry) = self.wrap.geometry() else {
            return false;
        };
        let Some((anchor, head)) = self.active else {
            return false;
        };
        let moved = geometry.move_pos(head, motion);
        if moved == head {
            return false;
        }
        self.active = Some((anchor, moved));
        true
    }

    /// Select the whole cached content, dropping committed regions.
    pub fn select_all(&mut self) {
        self.frozen.clear();
        self.active = self.wrap.geometry().and_then(PanelGeometry::select_all);
    }

    /// Freeze the active region and start over.
    pub fn commit(&mut self) {
        if let Some((anchor, head)) = self.active.take() {
            if anchor != head {
                self.frozen.push((anchor, head));
            }
        }
    }

    /// Clear every selection but keep the content.
    pub fn clear_selection(&mut self) {
        self.active = None;
        self.frozen.clear();
    }

    /// Whether any selection exists.
    pub fn has_selection(&self) -> bool {
        self.active.is_some() || !self.frozen.is_empty()
    }

    /// Committed regions followed by the active one, oldest first.
    fn regions(&self) -> Vec<PanelRegion> {
        let mut regions = self.frozen.clone();
        if let Some(region) = self.active {
            if region.0 != region.1 {
                regions.push(region);
            }
        }
        regions
    }

    /// Visible plain-text rows for a viewport.
    pub fn visible_rows(&self, scroll: u16, height: u16) -> Vec<String> {
        self.wrap.visible_window(scroll, height)
    }

    /// Extracted text of every selected region, ordered by document
    /// position. Whitespace-only regions are dropped.
    pub fn selected_parts(&self) -> Vec<String> {
        let mut regions = self.regions();
        regions.sort_by_key(|(anchor, head)| crate::panel_select::ordered(*anchor, *head));
        regions
            .into_iter()
            .filter_map(|(anchor, head)| self.wrap.extract_text(anchor, head, Some(&self.exclude)))
            .collect()
    }

    /// Extracted text of every selected region joined with newlines.
    pub fn selected_text(&self) -> Option<String> {
        let parts = self.selected_parts();
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    /// Highlight cells for every selected region in absolute terminal
    /// coordinates.
    pub fn highlight_cells(&self, area: Rect, scroll: u16) -> Vec<(u16, u16, u16)> {
        self.regions()
            .into_iter()
            .flat_map(|(anchor, head)| self.wrap.highlight_cells(anchor, head, area, scroll))
            .collect()
    }

    /// Wrapped row of the active cursor inside the cached layout.
    pub fn head_row(&self) -> Option<usize> {
        let head = self.head()?;
        self.wrap
            .geometry()
            .map(|geometry| geometry.textpos_to_row_col(head).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel_select::TextPos;

    fn panel_for(text: &str, width: usize) -> SelectablePanel {
        let mut panel = SelectablePanel::new();
        panel.set_plain(Arc::from(text), width);
        panel
    }

    #[test]
    fn plain_content_selects_the_dragged_range() {
        let mut panel = panel_for("hello world\nsecond line", 40);
        let area = Rect::new(0, 0, 40, 10);
        assert!(panel.begin((0, 0), area, 0));
        assert!(panel.extend((4, 0), area, 0));
        assert_eq!(panel.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn scroll_is_applied_when_mapping_cells() {
        let mut panel = panel_for("alpha\nbeta\ngamma", 40);
        let area = Rect::new(0, 0, 40, 2);
        assert!(panel.begin((0, 0), area, 1));
        assert!(panel.extend((4, 0), area, 1));
        assert_eq!(panel.selected_text().as_deref(), Some("beta"));
    }

    #[test]
    fn styled_rows_project_to_plain_text() {
        use ratatui::style::{Modifier, Style};
        let rows = vec![
            Line::from(ratatui::text::Span::styled(
                "bold",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from("plain"),
        ];
        let mut panel = SelectablePanel::new();
        panel.set_styled(&rows, 20);
        panel.select_all();
        assert_eq!(panel.selected_text().as_deref(), Some("bold\nplain"));
        assert_eq!(panel.styled_rows().len(), 2);
    }

    #[test]
    fn committed_regions_keep_disjoint_selections() {
        let mut panel = panel_for("alpha\nbeta\ngamma", 40);
        let area = Rect::new(0, 0, 40, 10);
        assert!(panel.begin((0, 0), area, 0));
        assert!(panel.extend((4, 0), area, 0));
        assert!(panel.begin_region((0, 2), area, 0));
        assert!(panel.extend((4, 2), area, 0));
        let parts = panel.selected_parts();
        assert_eq!(parts, vec!["alpha".to_string(), "gamma".to_string()]);
        assert_eq!(
            panel.selected_text().as_deref(),
            Some("alpha\ngamma"),
            "regions join in document order"
        );
    }

    #[test]
    fn motion_moves_the_active_cursor() {
        let mut panel = panel_for("hello\nworld", 40);
        panel.select_all();
        assert_eq!(panel.head(), Some(TextPos::new(1, 5)));
        assert!(panel.extend_motion(Motion::Up));
        assert_eq!(panel.head(), Some(TextPos::new(0, 5)));
        assert!(!panel.extend_motion(Motion::Up));
    }

    #[test]
    fn resize_keeps_the_logical_selection() {
        let mut panel = panel_for("hello world wrapped", 8);
        let area = Rect::new(0, 0, 8, 10);
        assert!(panel.begin((6, 0), area, 0));
        assert!(panel.extend((7, 1), area, 0));
        assert_eq!(panel.selected_text().as_deref(), Some("world wrap"));
        panel.set_plain(Arc::from("hello world wrapped"), 40);
        assert_eq!(
            panel.selected_text().as_deref(),
            Some("world wrap"),
            "same characters stay selected after a rewrap"
        );
    }
}
