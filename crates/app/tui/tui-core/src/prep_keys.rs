//! Cache keys and capacity policy for scrollback preparation.
//!
//! The prepared scrollback (see `prep_cache`) is keyed by terminal width and
//! a private content version. Width changes invalidate the layout while
//! content changes advance through incremental append/prepend paths. Theme
//! and animation changes never invalidate the layout: the discretized
//! animation tick lives in the redraw snapshot and the render view, not in
//! this key.

/// Key identifying one prepared scrollback revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollPrepKey {
    /// Terminal width in columns used for wrapping.
    pub width: u16,
    /// Private content version bumped on every scrollback mutation.
    pub version: u64,
}

/// Soft byte budget for the cached display rows. Oversize content keeps its
/// single cache entry and reports degraded mode via `is_oversize` instead of
/// growing additional entries.
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
}
