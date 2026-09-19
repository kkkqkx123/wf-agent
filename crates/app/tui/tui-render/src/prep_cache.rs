//! Incremental preparation cache for the scrollback area.
//!
//! The interactive draw path used to rewrap every history line on each frame.
//! This cache keeps the wrapped display rows plus a per-source index so steady
//! streaming frames only lay out the newly added source lines. Width changes
//! trigger a full relayout; theme and animation changes never invalidate the
//! layout. The in-flight streaming line is intentionally excluded and rendered
//! separately each frame.

use ratatui::text::Line;

use tui_components::transcript::HistoryLine;
use tui_core::prep_keys::{
    estimate_bytes, is_oversize, AuxMapKey, ScrollPrepKey, PREP_CACHE_MAX_BYTES,
};

/// Display-row interval belonging to one source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRange {
    /// Start offset inside the flat display-row array.
    pub start: usize,
    /// Number of display rows for this source line.
    pub len: usize,
}

/// Maximum cached messages in the row-level cache.
pub const ROW_CACHE_MAX_ENTRIES: usize = 512;
/// Maximum entries in the auxiliary rendered-to-logical map.
pub const AUX_MAP_MAX_ENTRIES: usize = 2048;
/// Retained oversize frames: long sessions keep their large transcript warm.
pub const OVERSIZE_CACHE_MAX_ENTRIES: usize = 2;

fn oldest_key(map: &std::collections::HashMap<AuxMapKey, CachedRows>) -> Option<AuxMapKey> {
    map.iter()
        .min_by_key(|(_, entry)| entry.generation)
        .map(|(key, _)| *key)
}

fn oldest_aux_key(map: &std::collections::HashMap<u64, GenerationalAux>) -> Option<u64> {
    map.iter()
        .min_by_key(|(_, entry)| entry.generation)
        .map(|(key, _)| *key)
}

/// Retained oversize preparation: a bounded queue of large frames kept warm
/// so a long session does not lose its single large transcript to eviction.
#[derive(Debug, Clone, Default)]
pub struct OversizeCache {
    entries: std::collections::VecDeque<(ScrollPrepKey, Vec<Line<'static>>)>,
}

impl OversizeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: ScrollPrepKey, rows: Vec<Line<'static>>) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(pos);
        }
        while self.entries.len() >= OVERSIZE_CACHE_MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back((key, rows));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keep at most the newest entry when the global byte budget is blown.
    /// Eviction order is FIFO: oldest oversize frames drop first.
    pub fn retain_newest(&mut self) {
        while self.entries.len() > 1 {
            self.entries.pop_front();
        }
    }
}

/// Row-level message cache: message identity plus width → laid-out display
/// rows. Unlike the scrollback preparation (keyed by content version), this
/// layer survives replace cycles, so reloading identical content lays out
/// nothing. Width changes invalidate every entry because rows are
/// width-specific.
#[derive(Debug, Clone, Default)]
pub struct RowCache {
    entries: std::collections::HashMap<AuxMapKey, CachedRows>,
    generation: u64,
}

#[derive(Debug, Clone)]
struct CachedRows {
    generation: u64,
    rows: Vec<Line<'static>>,
}

impl RowCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cached rows for `key`, if present.
    pub fn get(&self, key: AuxMapKey) -> Option<&[Line<'static>]> {
        self.entries.get(&key).map(|entry| entry.rows.as_slice())
    }

    /// Store laid-out rows. When full, evict the oldest generation instead
    /// of dropping the whole cache, so steady content keeps its hits while
    /// churned identities age out.
    pub fn insert(&mut self, key: AuxMapKey, rows: Vec<Line<'static>>) {
        self.generation = self.generation.wrapping_add(1);
        if self.entries.len() >= ROW_CACHE_MAX_ENTRIES && !self.entries.contains_key(&key) {
            if let Some(oldest) = oldest_key(&self.entries) {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            CachedRows {
                generation: self.generation,
                rows,
            },
        );
    }

    /// Drop all entries (width change).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached messages.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Auxiliary rendered-to-logical entry: how many display rows one message
/// produced at a width. Used for text selection and row-count mapping
/// without re-parsing the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxEntry {
    /// Display rows produced for the message.
    pub display_len: usize,
    /// Width the entry was recorded at.
    pub width: u16,
}

/// Auxiliary map from message identity to [`AuxEntry`]. It is never cleared
/// by body-cache invalidation (that is its purpose: surviving a last-message
/// edit without re-parsing the same assistant message); entries age out by
/// oldest generation once the capacity cap is reached.
#[derive(Debug, Clone, Default)]
pub struct AuxLineMap {
    entries: std::collections::HashMap<u64, GenerationalAux>,
    generation: u64,
}

#[derive(Debug, Clone, Copy)]
struct GenerationalAux {
    generation: u64,
    entry: AuxEntry,
}

impl AuxLineMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the display length of `message_hash` at `width`.
    pub fn record(&mut self, message_hash: u64, width: u16, display_len: usize) {
        self.generation = self.generation.wrapping_add(1);
        if self.entries.len() >= AUX_MAP_MAX_ENTRIES && !self.entries.contains_key(&message_hash) {
            if let Some(oldest) = oldest_aux_key(&self.entries) {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            message_hash,
            GenerationalAux {
                generation: self.generation,
                entry: AuxEntry { display_len, width },
            },
        );
    }

    /// Display length recorded for `message_hash` at `width`, if any.
    pub fn display_len(&self, message_hash: u64, width: u16) -> Option<usize> {
        self.entries.get(&message_hash).and_then(|wrapper| {
            if wrapper.entry.width == width {
                Some(wrapper.entry.display_len)
            } else {
                None
            }
        })
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use tui_components::transcript::{LineState, Role};

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
        let shift = cache.sync_prepend(&full, 1, 10, 2);
        assert_eq!(cache.last_layout_count(), 1);
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
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

    #[test]
    fn replace_identical_content_reuses_row_cache() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&full, 10, 1);
        assert_eq!(cache.last_layout_count(), 2);
        assert!(!cache.row_cache().is_empty());
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
    }

    #[test]
    fn aux_map_survives_replace_without_reparsing() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("assistant answer one two three")];
        cache.sync_replace(&full, 10, 1);
        let identity = full[0].identity();
        let recorded = cache.aux_map().display_len(identity, 10);
        assert_eq!(recorded, Some(cache.total_rows()));
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.aux_map().display_len(identity, 10), recorded);
        assert_eq!(cache.aux_map().display_len(identity, 30), None);
    }

    #[test]
    fn width_change_clears_row_cache_but_text_matches() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("some longer content for wrapping")];
        cache.sync_replace(&full, 10, 1);
        assert!(!cache.row_cache().is_empty());
        cache.sync_replace(&full, 30, 2);
        assert_eq!(cached_text(&cache), full_layout(&full, 30));
        assert_eq!(cache.last_layout_count(), 1);
    }

    #[test]
    fn row_cache_evicts_oldest_generation_when_full() {
        let mut row_cache = RowCache::new();
        for i in 0..=ROW_CACHE_MAX_ENTRIES {
            row_cache.insert(
                AuxMapKey::new(i as u64, 80),
                vec![Line::from(format!("row {i}"))],
            );
        }
        assert!(row_cache.len() <= ROW_CACHE_MAX_ENTRIES);
        assert!(row_cache
            .get(AuxMapKey::new(ROW_CACHE_MAX_ENTRIES as u64, 80))
            .is_some());
    }

    #[test]
    fn hidden_mismatch_forces_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let tail = vec![line("tail one two three"), line("tail two")];
        cache.sync_replace_with_hidden(&tail, 10, 1, 0);
        let mut full = vec![line("head line newcomer")];
        full.extend(tail.clone());
        let shift = cache.sync_prepend_with_hidden(&full, 1, 10, 2, 3);
        assert_eq!(cache.hidden(), 3);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
    }

    #[test]
    fn generational_eviction_preserves_recent_hits() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&full, 10, 1);
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert!(!cache.row_cache().is_empty());
    }

    #[test]
    fn tail_edit_rebuilds_only_body() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("head pinned"), line("body one"), line("tail old")];
        cache.sync_replace(&full, 12, 1);
        cache.set_header_len(1);
        let mut edited = full.clone();
        edited[2] = line("tail new content here");
        cache.sync_append_with_hidden(&edited, 12, 2, 0);
        assert_eq!(cached_text(&cache), full_layout(&edited, 12));
        assert_eq!(cache.last_layout_count(), 1);
        assert_index_consistent(&cache);
    }

    #[test]
    fn header_body_keys_split_hidden() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("head"), line("body")];
        cache.sync_replace_with_hidden(&full, 10, 7, 3);
        assert_eq!(cache.header_key().width, 10);
        assert_eq!(cache.header_key().version, 7);
        assert_eq!(cache.body_key().hidden, 3);
        assert_eq!(cache.key().body_key(), cache.body_key());
        assert_eq!(cache.key().header_key(), cache.header_key());
    }

    #[test]
    fn discontinuous_seq_forces_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let mut tail = vec![line("old tail one"), line("old tail two")];
        tail[0].set_seq_no(10);
        tail[1].set_seq_no(11);
        cache.sync_replace(&tail, 12, 1);
        let mut head = vec![line("new head")];
        head[0].set_seq_no(1);
        let mut full = head;
        full.extend(tail.clone());
        let shift = cache.sync_prepend_with_hidden(&full, 1, 12, 2, 0);
        assert_eq!(cached_text(&cache), full_layout(&full, 12));
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
        assert_index_consistent(&cache);
    }

    #[test]
    fn streaming_settle_hits_row_cache() {
        let mut cache = PreparedScrollback::new();
        let streaming =
            HistoryLine::new_with_role("same text", LineState::Streaming, Role::Default);
        let committed =
            HistoryLine::new_with_role("same text", LineState::Committed, Role::Default);
        assert_eq!(streaming.identity(), committed.identity());
        let full = vec![committed];
        cache.sync_replace(&full, 12, 1);
        assert_eq!(cache.last_layout_count(), 1);
        cache.sync_replace(&full, 12, 2);
        assert_eq!(cache.last_layout_count(), 0);
    }
}
