//! Cache keys and capacity policy for scrollback preparation.
//!
//! The prepared scrollback (see `prep_cache`) is keyed by terminal width and
//! a private content version. Width changes invalidate the layout while
//! content changes advance through incremental append/prepend paths. Theme
//! and animation changes never invalidate the layout: the discretized
//! animation tick lives in the redraw snapshot and the render view, not in
//! this key. Highlight backend and markdown dialect versions ride along so a
//! highlighter or dialect switch never reuses stale laid-out rows.

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
}

impl ScrollPrepKey {
    /// Current dialect and highlight inputs for a fresh key.
    pub fn inputs() -> (u64, u64) {
        (DIALECT_VERSION, HIGHLIGHT_VERSION)
    }

    /// Header portion of the key: pinned head rows depend only on width and
    /// version, never on hidden compaction or streaming state. Dialect and
    /// highlight inputs still participate: they change laid-out rows.
    pub fn header_key(self) -> HeaderPrepKey {
        HeaderPrepKey {
            width: self.width,
            version: self.version,
            dialect: self.dialect,
            highlight: self.highlight,
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
        }
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
/// affected messages. Dialect and highlight inputs participate so a backend
/// switch never reuses stale rows.
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
}
