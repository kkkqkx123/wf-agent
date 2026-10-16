//! Cache keys and capacity policy for scrollback preparation.
//!
//! The prepared scrollback (see `prep_cache`) is keyed by terminal width and
//! a private content version. Width changes invalidate the layout while
//! content changes advance through incremental append/prepend paths. Theme
//! and animation changes never invalidate the layout: the discretized
//! animation tick lives in the redraw snapshot and the render view, not in
//! this key. Highlight backend, markdown dialect, spacing and latex inputs
//! ride along so a backend, dialect or render-mode switch never reuses stale
//! laid-out rows. Frame-level inputs (height, overlay, diagram pane, image
//! collection, expanded level, aspect bucket) live in the redraw snapshot and
//! force a full frame without relaying out rows.

use tui_markdown::markdown::dialect::DIALECT_VERSION;
use tui_markdown::markdown::highlight::HIGHLIGHT_VERSION;

/// Key identifying one prepared scrollback revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollPrepKey {
    /// Terminal width in columns used for wrapping.
    pub width: u16,
    /// Private content version bumped on every scrollback mutation.
    pub version: u64,
    /// Compacted hidden prompt count: prepending older history must preserve
    /// absolute numbering continuity, so a hidden-count mismatch forces a
    /// full relayout instead of reusing numbered rows.
    pub hidden: u64,
    /// Canonical markdown dialect version (see `dialect::DIALECT_VERSION`).
    pub dialect: u64,
    /// Code highlight backend version (see `highlight::HIGHLIGHT_VERSION`).
    pub highlight: u64,
    /// Spacing mode affecting row gaps.
    pub spacing: u8,
    /// Chart display mode for diagram fences.
    pub chart: u8,
    /// Latex rendering mode (unicode now, image later).
    pub latex: u8,
}

impl ScrollPrepKey {
    /// Current dialect and highlight inputs for a fresh key.
    pub fn inputs() -> (u64, u64) {
        (DIALECT_VERSION, HIGHLIGHT_VERSION)
    }

    /// Header portion of the key: pinned head rows depend only on width and
    /// version, never on hidden compaction or streaming state. Dialect,
    /// highlight, spacing, chart and latex inputs still participate: they change
    /// laid-out rows.
    pub fn header_key(self) -> HeaderPrepKey {
        HeaderPrepKey {
            width: self.width,
            version: self.version,
            dialect: self.dialect,
            highlight: self.highlight,
            spacing: self.spacing,
            chart: self.chart,
            latex: self.latex,
        }
    }

    /// Body portion of the key: tail rows additionally depend on hidden
    /// compaction. The streaming tail never enters this key; it only joins
    /// the frame assembly.
    pub fn body_key(self) -> BodyPrepKey {
        BodyPrepKey {
            width: self.width,
            version: self.version,
            hidden: self.hidden,
            dialect: self.dialect,
            highlight: self.highlight,
            spacing: self.spacing,
            chart: self.chart,
            latex: self.latex,
        }
    }

    /// True when the row geometry matches (everything except the version).
    /// Incremental paths reuse rows across versions under the same geometry.
    pub fn same_geometry(self, other: ScrollPrepKey) -> bool {
        self.width == other.width
            && self.hidden == other.hidden
            && self.dialect == other.dialect
            && self.highlight == other.highlight
            && self.spacing == other.spacing
            && self.chart == other.chart
            && self.latex == other.latex
    }
}

/// Header cache key: pinned head rows (session title, management header).
/// Editing the tail never changes this key, so the header reuses its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeaderPrepKey {
    /// Terminal width in columns used for wrapping.
    pub width: u16,
    /// Content version of the header section.
    pub version: u64,
    /// Canonical markdown dialect version.
    pub dialect: u64,
    /// Code highlight backend version.
    pub highlight: u64,
    /// Spacing mode affecting row gaps.
    pub spacing: u8,
    /// Chart display mode for diagram fences.
    pub chart: u8,
    /// Latex rendering mode.
    pub latex: u8,
}

/// Body cache key: scrollable tail rows. Hidden compaction participates
/// here; a hidden-count mismatch means absolute prompt numbers may have
/// shifted and forces a full relayout instead of an incremental splice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyPrepKey {
    /// Terminal width in columns used for wrapping.
    pub width: u16,
    /// Private content version bumped on every scrollback mutation.
    pub version: u64,
    /// Compacted hidden prompt count.
    pub hidden: u64,
    /// Canonical markdown dialect version.
    pub dialect: u64,
    /// Code highlight backend version.
    pub highlight: u64,
    /// Spacing mode affecting row gaps.
    pub spacing: u8,
    /// Chart display mode for diagram fences.
    pub chart: u8,
    /// Latex rendering mode.
    pub latex: u8,
}

/// Render mode bits folded into the full-frame identity (overlay, diff and
/// diagram modes). Body layout ignores them; frame identity does not.
pub const RENDER_MODE_OVERLAY: u8 = 0b0001;
/// Diff-mode rendering flag for the frame identity.
pub const RENDER_MODE_DIFF: u8 = 0b0010;
/// Diagram-pane-visible flag for the frame identity.
pub const RENDER_MODE_DIAGRAM: u8 = 0b0100;

/// Identity of one cached message row: content hash plus layout width.
/// Width changes invalidate every entry; content changes only miss the
/// affected messages. Dialect, highlight, spacing, chart and latex inputs
/// participate so a backend or mode switch never reuses stale rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuxMapKey {
    /// Content identity of the message (see `HistoryLine::identity`).
    pub message_hash: u64,
    /// Layout width in columns.
    pub width: u16,
    /// Canonical markdown dialect version.
    pub dialect: u64,
    /// Code highlight backend version.
    pub highlight: u64,
    /// Spacing mode affecting row gaps.
    pub spacing: u8,
    /// Chart display mode for diagram fences.
    pub chart: u8,
    /// Latex rendering mode.
    pub latex: u8,
}

impl AuxMapKey {
    /// New auxiliary-map identity under the current dialect and highlight
    /// backend.
    pub fn new(message_hash: u64, width: u16) -> Self {
        Self {
            message_hash,
            width,
            dialect: DIALECT_VERSION,
            highlight: HIGHLIGHT_VERSION,
            spacing: 0,
            chart: 0,
            latex: 0,
        }
    }

    /// New identity under explicit row modes.
    pub fn new_with_modes(message_hash: u64, width: u16, spacing: u8, latex: u8) -> Self {
        Self {
            message_hash,
            width,
            dialect: DIALECT_VERSION,
            highlight: HIGHLIGHT_VERSION,
            spacing,
            chart: 0,
            latex,
        }
    }

    /// New identity under explicit render modes (spacing, chart, latex).
    pub fn new_with_render_modes(
        message_hash: u64,
        width: u16,
        spacing: u8,
        chart: u8,
        latex: u8,
    ) -> Self {
        Self {
            message_hash,
            width,
            dialect: DIALECT_VERSION,
            highlight: HIGHLIGHT_VERSION,
            spacing,
            chart,
            latex,
        }
    }
}

/// Soft byte budget for the cached display rows. Oversize content keeps its
/// single cache entry and reports degraded mode via `is_oversize` instead of
/// growing additional entries.
///
/// Eviction order (oldest first): row-cache entries by generation, then
/// auxiliary map entries by generation, then the oversize queue by FIFO. The
/// live `rows` vector itself is never truncated here; oversize frames set the
/// degraded bit and stay readable while new inserts age out older identities.
pub const PREP_CACHE_MAX_BYTES: usize = 24 * 1024 * 1024;

/// Rough byte estimate for one display row (cells plus style overhead).
pub const BYTES_PER_ROW_ESTIMATE: usize = 256;

/// Estimate the cached byte size for `rows` display rows.
pub fn estimate_bytes(rows: usize) -> usize {
    rows.saturating_mul(BYTES_PER_ROW_ESTIMATE)
}

/// True when the prepared content fits into the cache budget.
pub fn should_cache(rows: usize) -> bool {
    estimate_bytes(rows) <= PREP_CACHE_MAX_BYTES
}

/// True when the content is oversize and runs degraded on its single cache
/// entry instead of growing additional entries.
pub fn is_oversize(rows: usize) -> bool {
    !should_cache(rows)
}

/// Body-tier frame budget: message-level preparation entries.
pub const BODY_CACHE_MAX_ENTRIES: usize = 8;
/// Body-tier byte budget.
pub const BODY_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Full-frame tier entry budget.
pub const FULL_PREP_CACHE_MAX_ENTRIES: usize = 4;
/// Full-frame tier byte budget.
pub const FULL_PREP_CACHE_MAX_BYTES: usize = 24 * 1024 * 1024;
/// Oversize queue length shared by both tiers.
pub const PREP_OVERSIZED_MAX_ENTRIES: usize = 2;

/// True when `rows` display rows fit the body-tier byte budget.
pub fn body_should_cache(rows: usize) -> bool {
    estimate_bytes(rows) <= BODY_CACHE_MAX_BYTES
}

/// True when `rows` display rows fit the full-frame tier byte budget.
pub fn full_should_cache(rows: usize) -> bool {
    estimate_bytes(rows) <= FULL_PREP_CACHE_MAX_BYTES
}

/// Incremental reuse decision from message-boundary identity prefixes.
///
/// Pure over identity fingerprints: a shared head means tail append, a shared
/// tail means head prepend, anything else needs a full relayout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalBase {
    /// No shared boundary; rebuild fully.
    None,
    /// First `shared` identities match; lay out only the new tail.
    Tail { shared: usize },
    /// Last `shared` identities match; lay out only the new head.
    Prepend { shared: usize },
}

/// Find the incremental base between cached and current identities.
pub fn find_incremental_base(cached: &[u64], full: &[u64]) -> IncrementalBase {
    if cached.is_empty() {
        return IncrementalBase::Tail { shared: 0 };
    }
    if full.len() >= cached.len() && full[..cached.len()] == cached[..] {
        return IncrementalBase::Tail {
            shared: cached.len(),
        };
    }
    if full.len() >= cached.len() && full[full.len() - cached.len()..] == cached[..] {
        return IncrementalBase::Prepend {
            shared: cached.len(),
        };
    }
    IncrementalBase::None
}

/// One retained frame identity for LRU lookup without duplicating rows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryEntry {
    key: ScrollPrepKey,
    identities: Vec<u64>,
    approx_bytes: usize,
}

/// Bounded LRU of frame identities with exact-hit promotion, prefix-match
/// lookup and a separate oversized queue. Stores identities only; rows stay
/// with the live preparation. Oversized frames never evict normal entries.
#[derive(Debug, Clone, Default)]
pub struct FrameHistory {
    entries: std::collections::VecDeque<HistoryEntry>,
    oversized: std::collections::VecDeque<HistoryEntry>,
    max_entries: usize,
    max_bytes: usize,
}

impl FrameHistory {
    /// Empty history with explicit budgets.
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: std::collections::VecDeque::new(),
            oversized: std::collections::VecDeque::new(),
            max_entries: max_entries.max(1),
            max_bytes,
        }
    }

    /// Body-tier history (eight frames, thirty-two megabytes).
    pub fn new_body() -> Self {
        Self::new(BODY_CACHE_MAX_ENTRIES, BODY_CACHE_MAX_BYTES)
    }

    /// Full-frame tier history (four frames, twenty-four megabytes).
    pub fn new_full() -> Self {
        Self::new(FULL_PREP_CACHE_MAX_ENTRIES, FULL_PREP_CACHE_MAX_BYTES)
    }

    /// Normal entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no normal entry is retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Oversized queue length.
    pub fn oversized_len(&self) -> usize {
        self.oversized.len()
    }

    /// Record a frame identity, promoting exact hits to the front. Oversized
    /// frames go to the bounded oversized queue and never evict normal
    /// entries.
    pub fn record(&mut self, key: ScrollPrepKey, identities: Vec<u64>, approx_bytes: usize) {
        if approx_bytes > self.max_bytes {
            if let Some(pos) = self.oversized.iter().position(|entry| entry.key == key) {
                self.oversized.remove(pos);
            }
            while self.oversized.len() >= PREP_OVERSIZED_MAX_ENTRIES {
                self.oversized.pop_front();
            }
            self.oversized.push_back(HistoryEntry {
                key,
                identities,
                approx_bytes,
            });
            return;
        }
        if let Some(pos) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries.remove(pos);
        }
        while self.entries.len() >= self.max_entries {
            self.entries.pop_back();
        }
        let bytes: usize = approx_bytes;
        while !self.entries.is_empty() && bytes.saturating_add(self.stored_bytes()) > self.max_bytes
        {
            self.entries.pop_back();
        }
        self.entries.push_front(HistoryEntry {
            key,
            identities,
            approx_bytes,
        });
    }

    /// Bytes currently retained in normal entries.
    fn stored_bytes(&self) -> usize {
        self.entries.iter().map(|entry| entry.approx_bytes).sum()
    }

    /// Exact-hit lookup promoting the entry to the front.
    pub fn find_exact(&mut self, key: &ScrollPrepKey) -> bool {
        let Some(pos) = self.entries.iter().position(|entry| &entry.key == key) else {
            return self.oversized.iter().any(|entry| &entry.key == key);
        };
        if pos > 0 {
            if let Some(entry) = self.entries.remove(pos) {
                self.entries.push_front(entry);
            }
        }
        true
    }

    /// Best incremental base for `full` among geometry-matching entries,
    /// most-recent first. Returns the entry key plus the reuse decision.
    pub fn find_base(
        &self,
        full: &[u64],
        geometry: &ScrollPrepKey,
    ) -> Option<(ScrollPrepKey, IncrementalBase)> {
        for entry in &self.entries {
            if !entry.key.same_geometry(*geometry) {
                continue;
            }
            match find_incremental_base(&entry.identities, full) {
                IncrementalBase::None => continue,
                base => return Some((entry.key, base)),
            }
        }
        None
    }
}

/// Hash the streaming prefix for frame-key identity (FNV-1a, no dep).
pub fn hash_prefix(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_content_fits_budget() {
        assert!(should_cache(100));
        assert!(!is_oversize(100));
    }

    #[test]
    fn huge_content_is_oversize() {
        let rows = PREP_CACHE_MAX_BYTES / BYTES_PER_ROW_ESTIMATE + 1;
        assert!(is_oversize(rows));
    }

    #[test]
    fn prefix_hash_is_stable() {
        assert_eq!(hash_prefix("abc"), hash_prefix("abc"));
        assert_ne!(hash_prefix("abc"), hash_prefix("abd"));
    }

    #[test]
    fn aux_key_distinguishes_content_and_width() {
        assert_eq!(AuxMapKey::new(1, 80), AuxMapKey::new(1, 80));
        assert_ne!(AuxMapKey::new(1, 80), AuxMapKey::new(2, 80));
        assert_ne!(AuxMapKey::new(1, 80), AuxMapKey::new(1, 100));
    }

    #[test]
    fn keys_carry_dialect_and_highlight_inputs() {
        let key = AuxMapKey::new(1, 80);
        assert_eq!(key.dialect, super::DIALECT_VERSION);
        assert_eq!(key.highlight, super::HIGHLIGHT_VERSION);
        let (dialect, highlight) = super::ScrollPrepKey::inputs();
        assert_eq!(dialect, super::DIALECT_VERSION);
        assert_eq!(highlight, super::HIGHLIGHT_VERSION);
    }

    #[test]
    fn row_modes_participate_in_keys() {
        let plain = AuxMapKey::new(7, 80);
        let latex = AuxMapKey::new_with_modes(7, 80, 0, 1);
        assert_ne!(plain, latex);
        assert_eq!(latex.spacing, 0);
        assert_eq!(latex.latex, 1);
        let chart = AuxMapKey::new_with_render_modes(7, 80, 0, 1, 0);
        assert_ne!(plain, chart);
        assert_eq!(chart.chart, 1);
    }

    #[test]
    fn geometry_mismatch_blocks_reuse() {
        let (dialect, highlight) = super::ScrollPrepKey::inputs();
        let base = super::ScrollPrepKey {
            width: 80,
            version: 3,
            hidden: 0,
            dialect,
            highlight,
            spacing: 0,
            chart: 0,
            latex: 0,
        };
        let latex_changed = super::ScrollPrepKey { latex: 1, ..base };
        assert!(!base.same_geometry(latex_changed));
        assert!(base.same_geometry(super::ScrollPrepKey { version: 9, ..base }));
        assert_eq!(base.header_key().latex, 0);
        assert_eq!(latex_changed.body_key().latex, 1);
        let chart_changed = super::ScrollPrepKey { chart: 1, ..base };
        assert!(!base.same_geometry(chart_changed));
        assert_eq!(chart_changed.header_key().chart, 1);
    }

    #[test]
    fn dual_budgets_split_body_and_full() {
        assert!(super::body_should_cache(100));
        assert!(super::full_should_cache(100));
        let body_rows = super::BODY_CACHE_MAX_BYTES / super::BYTES_PER_ROW_ESTIMATE;
        assert!(super::body_should_cache(body_rows));
        assert!(!super::full_should_cache(body_rows));
        assert!(!super::body_should_cache(body_rows + 1));
    }

    #[test]
    fn incremental_base_detects_tail_and_prepend() {
        use super::{find_incremental_base, IncrementalBase};
        assert_eq!(
            find_incremental_base(&[], &[1, 2]),
            IncrementalBase::Tail { shared: 0 }
        );
        assert_eq!(
            find_incremental_base(&[1, 2], &[1, 2, 3]),
            IncrementalBase::Tail { shared: 2 }
        );
        assert_eq!(
            find_incremental_base(&[2, 3], &[1, 2, 3]),
            IncrementalBase::Prepend { shared: 2 }
        );
        assert_eq!(
            find_incremental_base(&[1, 9], &[1, 2, 3]),
            IncrementalBase::None
        );
    }

    #[test]
    fn history_promotes_exact_hits_and_matches_prefix() {
        use super::FrameHistory;
        let (dialect, highlight) = super::ScrollPrepKey::inputs();
        let key = |version: u64| super::ScrollPrepKey {
            width: 80,
            version,
            hidden: 0,
            dialect,
            highlight,
            spacing: 0,
            chart: 0,
            latex: 0,
        };
        let mut history = FrameHistory::new_body();
        history.record(key(1), vec![1, 2], 512);
        history.record(key(2), vec![1, 2, 3], 768);
        assert!(history.find_exact(&key(2)));
        assert_eq!(history.len(), 2);
        let probe = key(99);
        let found = history.find_base(&[1, 2, 3, 4], &probe);
        assert!(found.is_some());
        let (_, base) = found.expect("base");
        assert_eq!(base, super::IncrementalBase::Tail { shared: 3 });
    }

    #[test]
    fn history_oversized_never_evicts_normal() {
        use super::FrameHistory;
        let (dialect, highlight) = super::ScrollPrepKey::inputs();
        let key = |version: u64| super::ScrollPrepKey {
            width: 80,
            version,
            hidden: 0,
            dialect,
            highlight,
            spacing: 0,
            chart: 0,
            latex: 0,
        };
        let mut history = FrameHistory::new_full();
        history.record(key(1), vec![1], 256);
        history.record(key(2), vec![2], super::FULL_PREP_CACHE_MAX_BYTES + 1);
        assert_eq!(history.len(), 1);
        assert_eq!(history.oversized_len(), 1);
        assert!(history.find_exact(&key(1)));
    }
}
