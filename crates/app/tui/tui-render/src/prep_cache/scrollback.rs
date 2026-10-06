//! Cached preparation of the committed scrollback: a pinned header section
//! plus a scrollable body section with separate effective keys.

use ratatui::text::Line;

use tui_components::transcript::HistoryLine;
use tui_core::prep_keys::{
    estimate_bytes, is_oversize, AuxMapKey, ScrollPrepKey, PREP_CACHE_MAX_BYTES,
};

use super::aux_map::AuxLineMap;
use super::oversize::OversizeCache;
use super::row_cache::RowCache;
use super::row_range::RowRange;

/// Cached preparation of the committed scrollback.
///
/// The cache is split into a pinned header section (first `header_len`
/// source lines) and a scrollable body section. Header and body carry
/// separate effective keys: editing the tail only rebuilds the body while
/// header rows are reused. The streaming tail never enters either key; it
/// only joins the frame assembly.
#[derive(Debug, Clone, Default)]
pub struct PreparedScrollback {
    width: u16,
    version: u64,
    hidden: u64,
    source_len: usize,
    /// Pinned leading source lines forming the header section.
    header_len: usize,
    rows: Vec<Line<'static>>,
    index: Vec<RowRange>,
    /// Per-source-line identity fingerprints parallel to `index`; lets
    /// `sync_body_edit` find the first changed line without string compares.
    identities: Vec<u64>,
    last_layout_source_lines: usize,
    last_layout_rows: usize,
    row_cache: RowCache,
    aux_map: AuxLineMap,
    oversize: OversizeCache,
}

impl PreparedScrollback {
    /// Empty cache with a sane default width.
    pub fn new() -> Self {
        Self {
            width: 80,
            version: 0,
            hidden: 0,
            source_len: 0,
            header_len: 0,
            rows: Vec::new(),
            index: Vec::new(),
            identities: Vec::new(),
            last_layout_source_lines: 0,
            last_layout_rows: 0,
            row_cache: RowCache::new(),
            aux_map: AuxLineMap::new(),
            oversize: OversizeCache::new(),
        }
    }

    /// Cache key for the current revision.
    pub fn key(&self) -> ScrollPrepKey {
        ScrollPrepKey {
            width: self.width,
            version: self.version,
            hidden: self.hidden,
        }
    }

    /// Header-section effective key: width plus version. Tail edits bump the
    /// version but reuse header rows when the leading `header_len` identities
    /// are unchanged (checked in `sync_body_edit`).
    pub fn header_key(&self) -> tui_core::prep_keys::HeaderPrepKey {
        tui_core::prep_keys::HeaderPrepKey {
            width: self.width,
            version: self.version,
        }
    }

    /// Body-section effective key: width, version and hidden compaction. The
    /// streaming tail never participates; it only joins the frame assembly.
    pub fn body_key(&self) -> tui_core::prep_keys::BodyPrepKey {
        tui_core::prep_keys::BodyPrepKey {
            width: self.width,
            version: self.version,
            hidden: self.hidden,
        }
    }

    /// Pinned header source lines.
    pub fn header_len(&self) -> usize {
        self.header_len
    }

    /// Set the pinned header length (clamped to the cached source length).
    /// Changing it forces a full relayout on the next sync.
    pub fn set_header_len(&mut self, header_len: usize) {
        let clamped = header_len.min(self.source_len);
        if clamped != self.header_len {
            self.header_len = clamped;
            self.version = self.version.wrapping_add(1);
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

    /// Compacted hidden prompt count the cached rows were built from.
    pub fn hidden(&self) -> u64 {
        self.hidden
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

    /// Row-level message cache backing this preparation.
    pub fn row_cache(&self) -> &RowCache {
        &self.row_cache
    }

    /// Auxiliary rendered-to-logical map backing this preparation.
    pub fn aux_map(&self) -> &AuxLineMap {
        &self.aux_map
    }

    /// Lay out one source line, reusing the row cache on a hit. Returns the
    /// display rows and whether a real layout ran.
    fn lay_out_row(&mut self, source: &HistoryLine, width: u16) -> (Vec<Line<'static>>, bool) {
        let identity = source.identity();
        let key = AuxMapKey::new(identity, width);
        if let Some(cached) = self.row_cache.get(key).map(|rows| rows.to_vec()) {
            self.aux_map.record(identity, width, cached.len());
            return (cached, false);
        }
        let laid = source.display_lines(width);
        self.aux_map.record(identity, width, laid.len());
        self.row_cache.insert(key, laid.clone());
        (laid, true)
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
        self.sync_replace_with_hidden(full, width, version, self.hidden);
    }

    /// Replace with an explicit hidden-prompt count.
    pub fn sync_replace_with_hidden(
        &mut self,
        full: &[HistoryLine],
        width: u16,
        version: u64,
        hidden: u64,
    ) {
        self.relayout_all_with_hidden(full, width, version, hidden);
    }

    /// Append path: only the tail beyond the cached source length is laid out.
    /// A width change falls back to a full relayout.
    pub fn sync_append(&mut self, full: &[HistoryLine], width: u16, version: u64) {
        self.sync_append_with_hidden(full, width, version, self.hidden);
    }

    /// Append with an explicit hidden-prompt count.
    pub fn sync_append_with_hidden(
        &mut self,
        full: &[HistoryLine],
        width: u16,
        version: u64,
        hidden: u64,
    ) {
        if width != self.width || hidden != self.hidden {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return;
        }
        if full.len() < self.source_len {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return;
        }
        if full.len() == self.source_len {
            // Same length but a new version means an in-place tail edit.
            self.sync_body_edit(full, width, version, hidden);
            return;
        }
        let fresh = &full[self.source_len..];
        let mut laid_rows = 0usize;
        let mut misses = 0usize;
        for source in fresh {
            let start = self.rows.len();
            let (laid, miss) = self.lay_out_row(source, width);
            laid_rows += laid.len();
            misses += usize::from(miss);
            let len = laid.len();
            self.rows.extend(laid);
            self.index.push(RowRange { start, len });
            self.identities.push(source.identity());
        }
        self.source_len = full.len();
        self.version = version;
        self.hidden = hidden;
        self.header_len = self.header_len.min(self.source_len);
        self.last_layout_source_lines = misses;
        self.last_layout_rows = laid_rows;
        self.enforce_byte_budget();
        self.retain_oversize();
    }

    /// Tail-only rebuild for in-place edits: find the first body source line
    /// whose identity changed and relay out from there, reusing header rows
    /// and unchanged body prefixes via the row cache. Header rows (first
    /// `header_len` lines) are never rebuilt here; a header change falls back
    /// to a full relayout.
    pub fn sync_body_edit(&mut self, full: &[HistoryLine], width: u16, version: u64, hidden: u64) {
        if width != self.width || hidden != self.hidden || full.len() != self.source_len {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return;
        }
        let header_len = self.header_len.min(full.len());
        let mut first_changed = full.len();
        for (idx, source) in full.iter().enumerate() {
            // Identity fingerprints are the cheap row-equivalence signal: two
            // lines with the same identity at the same width lay out to the
            // same rows, so a fingerprint match means the cached rows are
            // reusable without comparing rendered text.
            let matches =
                self.identities.get(idx) == Some(&source.identity()) && idx < self.index.len();
            if !matches {
                first_changed = idx;
                break;
            }
        }
        if first_changed >= full.len() {
            self.version = version;
            self.hidden = hidden;
            self.last_layout_source_lines = 0;
            self.last_layout_rows = 0;
            return;
        }
        if first_changed < header_len {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return;
        }
        let base = self.index[first_changed].start;
        self.rows.truncate(base);
        self.index.truncate(first_changed);
        self.identities.truncate(first_changed);
        let mut laid_rows = 0usize;
        let mut misses = 0usize;
        for source in &full[first_changed..] {
            let start = self.rows.len();
            let (laid, miss) = self.lay_out_row(source, width);
            laid_rows += laid.len();
            misses += usize::from(miss);
            let len = laid.len();
            self.rows.extend(laid);
            self.index.push(RowRange { start, len });
            self.identities.push(source.identity());
        }
        self.version = version;
        self.hidden = hidden;
        self.last_layout_source_lines = misses;
        self.last_layout_rows = laid_rows;
        self.enforce_byte_budget();
        self.retain_oversize();
    }

    /// Prepend path: lay out the new head then splice it in front, shifting
    /// existing intervals so the visual anchor stays stable. Returns the
    /// number of new display rows so callers can compensate viewport offsets
    /// in display-row units.
    pub fn sync_prepend(
        &mut self,
        full: &[HistoryLine],
        added: usize,
        width: u16,
        version: u64,
    ) -> usize {
        self.sync_prepend_with_hidden(full, added, width, version, self.hidden)
    }

    /// Prepend with an explicit hidden-prompt count: a hidden-count mismatch
    /// means absolute numbering may have shifted, so fall back to a full
    /// relayout instead of reusing numbered rows. Sequenced lines additionally
    /// verify continuity: the last prepended `seq_no` must immediately precede
    /// the first cached `seq_no`, otherwise the page is discontinuous and the
    /// whole preparation is rebuilt.
    pub fn sync_prepend_with_hidden(
        &mut self,
        full: &[HistoryLine],
        added: usize,
        width: u16,
        version: u64,
        hidden: u64,
    ) -> usize {
        if width != self.width || hidden != self.hidden {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return self.head_display_rows(added);
        }
        let added = added.min(full.len());
        if self.source_len + added != full.len() {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return self.head_display_rows(added);
        }
        if added > 0 && !self.rows.is_empty() && !full.is_empty() {
            let boundary_new = full[added.min(full.len().saturating_sub(1))].seq_no();
            let boundary_old_new = full[added.saturating_sub(1)].seq_no();
            let cached_first = full.get(added).map(|l| l.seq_no()).unwrap_or(0);
            if boundary_old_new != 0 && cached_first != 0 && boundary_old_new + 1 != cached_first {
                self.relayout_all_with_hidden(full, width, version, hidden);
                return self.head_display_rows(added);
            }
            let _ = boundary_new;
        }
        let mut head_rows: Vec<Line<'static>> = Vec::new();
        let mut head_index: Vec<RowRange> = Vec::with_capacity(added);
        let mut head_identities: Vec<u64> = Vec::with_capacity(added);
        let mut misses = 0usize;
        for source in &full[..added] {
            let start = head_rows.len();
            let (laid, miss) = self.lay_out_row(source, width);
            misses += usize::from(miss);
            let len = laid.len();
            head_rows.extend(laid);
            head_index.push(RowRange { start, len });
            head_identities.push(source.identity());
        }
        let shift = head_rows.len();
        let mut rows = std::mem::take(&mut self.rows);
        let mut index = std::mem::take(&mut self.index);
        let mut identities = std::mem::take(&mut self.identities);
        for range in &mut index {
            range.start += shift;
        }
        head_rows.append(&mut rows);
        head_index.append(&mut index);
        head_identities.append(&mut identities);
        let laid_rows = shift;
        self.rows = head_rows;
        self.index = head_index;
        self.identities = head_identities;
        self.source_len = full.len();
        self.version = version;
        self.hidden = hidden;
        self.last_layout_source_lines = misses;
        self.last_layout_rows = laid_rows;
        self.retain_oversize();
        laid_rows
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
            self.identities.clear();
            self.source_len = 0;
            self.header_len = 0;
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
        self.identities.drain(0..dropped_sources);
        for range in &mut self.index {
            range.start -= dropped_rows;
        }
        self.source_len -= dropped_sources;
        self.header_len = self.header_len.saturating_sub(dropped_sources);
        self.version = version;
        self.last_layout_source_lines = 0;
        self.last_layout_rows = 0;
    }

    /// Draw-time safety net: fix width drift with a full relayout and repair
    /// any version or length drift the mutation sites may have missed.
    pub fn ensure_for_draw(&mut self, full: &[HistoryLine], width: u16, version: u64) -> bool {
        self.ensure_for_draw_with_hidden(full, width, version, self.hidden)
    }

    /// Draw-time safety net with an explicit hidden-prompt count.
    pub fn ensure_for_draw_with_hidden(
        &mut self,
        full: &[HistoryLine],
        width: u16,
        version: u64,
        hidden: u64,
    ) -> bool {
        if width != self.width
            || version != self.version
            || hidden != self.hidden
            || full.len() != self.source_len
        {
            self.relayout_all_with_hidden(full, width, version, hidden);
            return true;
        }
        false
    }

    /// Display rows contributed by the first `added` source lines under the
    /// current index. Used to report prepend shifts after fallback relayouts.
    fn head_display_rows(&self, added: usize) -> usize {
        self.index.iter().take(added).map(|range| range.len).sum()
    }

    /// Full relayout used for replace, width change, and correctness fallback.
    /// Row-cache hits are reused, so reloading identical content lays out
    /// nothing; a width change clears the width-specific row cache first.
    pub fn relayout_all(&mut self, full: &[HistoryLine], width: u16, version: u64) {
        self.relayout_all_with_hidden(full, width, version, self.hidden);
    }

    /// Full relayout with an explicit hidden-prompt count.
    pub fn relayout_all_with_hidden(
        &mut self,
        full: &[HistoryLine],
        width: u16,
        version: u64,
        hidden: u64,
    ) {
        if width != self.width {
            self.row_cache.clear();
        }
        let mut rows: Vec<Line<'static>> = Vec::new();
        let mut index: Vec<RowRange> = Vec::with_capacity(full.len());
        let mut identities: Vec<u64> = Vec::with_capacity(full.len());
        let mut misses = 0usize;
        for source in full {
            let start = rows.len();
            let (laid, miss) = self.lay_out_row(source, width);
            misses += usize::from(miss);
            let len = laid.len();
            rows.extend(laid);
            index.push(RowRange { start, len });
            identities.push(source.identity());
        }
        self.last_layout_source_lines = misses;
        self.last_layout_rows = rows.len();
        self.rows = rows;
        self.index = index;
        self.identities = identities;
        self.width = width;
        self.version = version;
        self.hidden = hidden;
        self.source_len = full.len();
        self.header_len = self.header_len.min(self.source_len);
        self.enforce_byte_budget();
        self.retain_oversize();
    }

    /// Enforce the global byte budget on auxiliary caches. Eviction order is
    /// oldest generation first: row-cache entries, then the oversize FIFO
    /// keeps at most one newest entry. The live `rows` vector stays readable;
    /// oversize frames only set the degraded bit. Callers invoke this after
    /// every mutation that grows cached state.
    fn enforce_byte_budget(&mut self) {
        if estimate_bytes(self.rows.len()) <= PREP_CACHE_MAX_BYTES {
            return;
        }
        self.row_cache.clear();
        self.oversize.retain_newest();
    }

    /// Oversize retention entry point (see `retain_oversize`).
    fn retain_oversize(&mut self) {
        if is_oversize(self.rows.len()) {
            let key = self.key();
            let rows = self.rows.clone();
            self.oversize.insert(key, rows);
        }
    }
}
