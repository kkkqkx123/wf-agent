//! Incremental preparation cache for the scrollback area.
//!
//! The interactive draw path used to rewrap every history line on each frame.
//! This cache keeps the wrapped display rows plus a per-source index so steady
//! streaming frames only lay out the newly added source lines. Width changes
//! trigger a full relayout; theme and animation changes never invalidate the
//! layout. The in-flight streaming line is intentionally excluded and rendered
//! separately each frame.

use ratatui::text::Line;

use crate::prep_keys::{is_oversize, ScrollPrepKey};
use crate::transcript::HistoryLine;

/// Display-row interval belonging to one source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRange {
    /// Start offset inside the flat display-row array.
    pub start: usize,
    /// Number of display rows for this source line.
    pub len: usize,
}

/// Cached preparation of the committed scrollback.
#[derive(Debug, Clone, Default)]
pub struct PreparedScrollback {
    width: u16,
    version: u64,
    source_len: usize,
    rows: Vec<Line<'static>>,
    index: Vec<RowRange>,
    last_layout_source_lines: usize,
    last_layout_rows: usize,
}

impl PreparedScrollback {
    /// Empty cache with a sane default width.
    pub fn new() -> Self {
        Self {
            width: 80,
            version: 0,
            source_len: 0,
            rows: Vec::new(),
            index: Vec::new(),
            last_layout_source_lines: 0,
            last_layout_rows: 0,
        }
    }

    /// Cache key for the current revision.
    pub fn key(&self) -> ScrollPrepKey {
        ScrollPrepKey {
            width: self.width,
            version: self.version,
        }
    }

    /// Width the cached rows were laid out at.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Content version the cached rows were built from.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Number of source lines currently laid out.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Total cached display rows.
    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    /// Source lines laid out by the most recent sync.
    pub fn last_layout_count(&self) -> usize {
        self.last_layout_source_lines
    }

    /// Display rows produced by the most recent sync.
    pub fn last_layout_rows(&self) -> usize {
        self.last_layout_rows
    }

    /// True when the cached content exceeds the byte budget and runs in
    /// degraded single-entry mode.
    pub fn is_degraded(&self) -> bool {
        is_oversize(self.rows.len())
    }

    /// All cached display rows.
    pub fn rows(&self) -> &[Line<'static>] {
        &self.rows
    }

    /// Per-source display intervals.
    pub fn index(&self) -> &[RowRange] {
        &self.index
    }

    /// Visible display-row window for the draw path.
    pub fn visible_range(&self, start: usize, len: usize) -> &[Line<'static>] {
        if start >= self.rows.len() {
            return &[];
        }
        let end = (start.saturating_add(len)).min(self.rows.len());
        &self.rows[start..end]
    }

    /// Display-row start for a source line, for viewport anchoring.
    pub fn source_row_start(&self, source: usize) -> Option<usize> {
        self.index.get(source).map(|range| range.start)
    }

    /// Replace the whole scrollback and lay it out fully.
    pub fn sync_replace(&mut self, full: &[HistoryLine], width: u16, version: u64) {
        self.relayout_all(full, width, version);
    }

    /// Append path: only the tail beyond the cached source length is laid out.
    /// A width change falls back to a full relayout.
    pub fn sync_append(&mut self, full: &[HistoryLine], width: u16, version: u64) {
        if width != self.width {
            self.relayout_all(full, width, version);
            return;
        }
        if full.len() < self.source_len {
            self.relayout_all(full, width, version);
            return;
        }
        let fresh = &full[self.source_len..];
        let mut laid_rows = 0usize;
        for source in fresh {
            let start = self.rows.len();
            let laid = source.display_lines(width);
            laid_rows += laid.len();
            let len = laid.len();
            self.rows.extend(laid);
            self.index.push(RowRange { start, len });
        }
        self.source_len = full.len();
        self.version = version;
        self.last_layout_source_lines = fresh.len();
        self.last_layout_rows = laid_rows;
    }

    /// Prepend path: lay out the new head then splice it in front, shifting
    /// existing intervals so the visual anchor stays stable.
    pub fn sync_prepend(&mut self, full: &[HistoryLine], added: usize, width: u16, version: u64) {
        if width != self.width {
            self.relayout_all(full, width, version);
            return;
        }
        let added = added.min(full.len());
        if self.source_len + added != full.len() {
            self.relayout_all(full, width, version);
            return;
        }
        let mut head_rows: Vec<Line<'static>> = Vec::new();
        let mut head_index: Vec<RowRange> = Vec::with_capacity(added);
        for source in &full[..added] {
            let start = head_rows.len();
            let laid = source.display_lines(width);
            let len = laid.len();
            head_rows.extend(laid);
            head_index.push(RowRange { start, len });
        }
        let shift = head_rows.len();
        let mut rows = std::mem::take(&mut self.rows);
        let mut index = std::mem::take(&mut self.index);
        for range in &mut index {
            range.start += shift;
        }
        head_rows.append(&mut rows);
        head_index.append(&mut index);
        let laid_rows = shift;
        self.rows = head_rows;
        self.index = head_index;
        self.source_len = full.len();
        self.version = version;
        self.last_layout_source_lines = added;
        self.last_layout_rows = laid_rows;
    }

    /// Trim path: drop the head intervals without relaying out the rest.
    pub fn sync_trim(&mut self, dropped_sources: usize, version: u64) {
        if dropped_sources == 0 {
            self.version = version;
            self.last_layout_source_lines = 0;
            self.last_layout_rows = 0;
            return;
        }
        if dropped_sources >= self.source_len {
            self.rows.clear();
            self.index.clear();
            self.source_len = 0;
            self.version = version;
            self.last_layout_source_lines = 0;
            self.last_layout_rows = 0;
            return;
        }
        let dropped_rows = if dropped_sources < self.index.len() {
            self.index[dropped_sources].start
        } else {
            self.rows.len()
        };
        self.rows.drain(0..dropped_rows);
        self.index.drain(0..dropped_sources);
        for range in &mut self.index {
            range.start -= dropped_rows;
        }
        self.source_len -= dropped_sources;
        self.version = version;
        self.last_layout_source_lines = 0;
        self.last_layout_rows = 0;
    }

    /// Draw-time safety net: fix width drift with a full relayout and repair
    /// any version or length drift the mutation sites may have missed.
    pub fn ensure_for_draw(&mut self, full: &[HistoryLine], width: u16, version: u64) -> bool {
        if width != self.width || version != self.version || full.len() != self.source_len {
            self.relayout_all(full, width, version);
            return true;
        }
        false
    }

    /// Full relayout used for replace, width change, and correctness fallback.
    pub fn relayout_all(&mut self, full: &[HistoryLine], width: u16, version: u64) {
        let mut rows: Vec<Line<'static>> = Vec::new();
        let mut index: Vec<RowRange> = Vec::with_capacity(full.len());
        for source in full {
            let start = rows.len();
            let laid = source.display_lines(width);
            let len = laid.len();
            rows.extend(laid);
            index.push(RowRange { start, len });
        }
        self.last_layout_source_lines = full.len();
        self.last_layout_rows = rows.len();
        self.rows = rows;
        self.index = index;
        self.width = width;
        self.version = version;
        self.source_len = full.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{LineState, Role};

    fn line(text: &str) -> HistoryLine {
        HistoryLine::new_with_role(text, LineState::Committed, Role::Default)
    }

    fn full_layout(full: &[HistoryLine], width: u16) -> Vec<String> {
        full.iter()
            .flat_map(|line| line.display_lines(width))
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn cached_text(cache: &PreparedScrollback) -> Vec<String> {
        cache
            .rows()
            .iter()
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn assert_index_consistent(cache: &PreparedScrollback) {
        let mut expected = 0usize;
        for range in cache.index() {
            assert_eq!(range.start, expected);
            expected += range.len;
        }
        assert_eq!(expected, cache.total_rows());
        assert_eq!(cache.index().len(), cache.source_len());
    }

    #[test]
    fn append_lays_out_only_new_lines() {
        let mut cache = PreparedScrollback::new();
        let first = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&first, 10, 1);
        assert_eq!(cache.last_layout_count(), 2);
        let mut full = first.clone();
        full.push(line("brand new tail line"));
        cache.sync_append(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 1);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
    }

    #[test]
    fn prepend_keeps_anchor_and_index() {
        let mut cache = PreparedScrollback::new();
        let tail = vec![line("tail one two three"), line("tail two")];
        cache.sync_replace(&tail, 10, 1);
        let old_start = cache.source_row_start(0).unwrap_or(0);
        let mut full = vec![line("head line newcomer")];
        full.extend(tail.clone());
        cache.sync_prepend(&full, 1, 10, 2);
        assert_eq!(cache.last_layout_count(), 1);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
        let shifted = cache.source_row_start(1).unwrap_or(0);
        assert!(shifted > old_start || old_start == 0);
    }

    #[test]
    fn trim_drops_head_without_relayout() {
        let mut cache = PreparedScrollback::new();
        let full: Vec<HistoryLine> = (0..5)
            .map(|i| line(&format!("line number {i} content")))
            .collect();
        cache.sync_replace(&full, 12, 1);
        cache.sync_trim(2, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert_eq!(cached_text(&cache), full_layout(&full[2..], 12));
        assert_index_consistent(&cache);
    }

    #[test]
    fn width_change_falls_back_to_full() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("some longer content for wrapping")];
        cache.sync_replace(&full, 10, 1);
        let narrow_rows = cache.total_rows();
        cache.sync_append(&full, 30, 2);
        assert_eq!(cached_text(&cache), full_layout(&full, 30));
        assert!(cache.total_rows() <= narrow_rows);
    }

    #[test]
    fn random_sequence_matches_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let mut full: Vec<HistoryLine> = Vec::new();
        let mut version = 0u64;
        let width = 14u16;
        // Deterministic operation mix: append, prepend, trim, replace.
        for step in 0..40usize {
            version += 1;
            match step % 5 {
                0 | 1 => {
                    full.push(line(&format!("append step {step} with words")));
                    cache.sync_append(&full, width, version);
                }
                2 => {
                    let head = vec![
                        line(&format!("head {step}")),
                        line(&format!("head2 {step}")),
                    ];
                    let mut next = head.clone();
                    next.extend(full.clone());
                    full = next;
                    cache.sync_prepend(&full, head.len(), width, version);
                }
                3 if full.len() > 3 => {
                    let drop = 2usize;
                    full.drain(0..drop);
                    cache.sync_trim(drop, version);
                }
                _ => {
                    full = vec![line(&format!("replaced at {step}")), line("fresh tail")];
                    cache.sync_replace(&full, width, version);
                }
            }
            assert_eq!(
                cached_text(&cache),
                full_layout(&full, width),
                "step {step}"
            );
            assert_index_consistent(&cache);
        }
    }
}
