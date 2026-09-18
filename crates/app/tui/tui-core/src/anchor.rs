//! Anchor stability: visual regression signal for scroll and streaming.
//!
//! Cost metrics prove frames are cheap; stability proves they look steady.
//! The recorder hashes each frame with blank rows marked, so anchor drift
//! across scroll and streaming frames becomes a comparable number.

/// Blank-row marker hash distinguishing empty rows from content.
pub const BLANK_ROW_HASH: u64 = 0;

/// One frame of anchor content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorFrame {
    pub row_hashes: Vec<u64>,
}

impl AnchorFrame {
    pub fn from_rows(rows: &[String]) -> Self {
        Self {
            row_hashes: rows
                .iter()
                .map(|row| {
                    if row.trim().is_empty() {
                        BLANK_ROW_HASH
                    } else {
                        hash_row(row)
                    }
                })
                .collect(),
        }
    }
}

fn hash_row(row: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in row.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == BLANK_ROW_HASH {
        hash = 1;
    }
    hash
}

/// Stability recorder comparing consecutive anchor frames.
#[derive(Debug, Default)]
pub struct AnchorStabilityRecorder {
    previous: Option<AnchorFrame>,
    pub stable_frames: u64,
    pub unstable_frames: u64,
}

impl AnchorStabilityRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame, returning true when the anchor matches the previous.
    pub fn record(&mut self, frame: AnchorFrame) -> bool {
        let stable = self.previous.as_ref().is_some_and(|prev| {
            prev.row_hashes.len() == frame.row_hashes.len()
                && prev
                    .row_hashes
                    .iter()
                    .zip(frame.row_hashes.iter())
                    .filter(|(a, b)| **a != BLANK_ROW_HASH && **b != BLANK_ROW_HASH)
                    .all(|(a, b)| a == b)
        });
        if self.previous.is_some() {
            if stable {
                self.stable_frames += 1;
            } else {
                self.unstable_frames += 1;
            }
        }
        self.previous = Some(frame);
        stable
    }

    pub fn total(&self) -> u64 {
        self.stable_frames + self.unstable_frames
    }

    pub fn stability(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 1.0;
        }
        self.stable_frames as f64 / total as f64
    }
}

/// Wrapped-to-logical line map supporting text selection without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct WrappedLineMap {
    wrapped_to_logical: Vec<usize>,
    logical_count: usize,
}

impl WrappedLineMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the map from per-logical-line display counts.
    pub fn build(display_counts: &[usize]) -> Self {
        let mut wrapped_to_logical = Vec::new();
        for (logical, count) in display_counts.iter().enumerate() {
            for _ in 0..*count {
                wrapped_to_logical.push(logical);
            }
        }
        Self {
            wrapped_to_logical,
            logical_count: display_counts.len(),
        }
    }

    pub fn logical_for_wrapped(&self, wrapped: usize) -> Option<usize> {
        self.wrapped_to_logical.get(wrapped).copied()
    }

    pub fn wrapped_len(&self) -> usize {
        self.wrapped_to_logical.len()
    }

    pub fn logical_len(&self) -> usize {
        self.logical_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_rows_use_marker_hash() {
        let frame = AnchorFrame::from_rows(&["hello".to_string(), "   ".to_string()]);
        assert_ne!(frame.row_hashes[0], BLANK_ROW_HASH);
        assert_eq!(frame.row_hashes[1], BLANK_ROW_HASH);
    }

    #[test]
    fn identical_frames_are_stable() {
        let mut recorder = AnchorStabilityRecorder::new();
        recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()]));
        assert!(recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()])));
        assert_eq!(recorder.stability(), 1.0);
    }

    #[test]
    fn changed_content_is_unstable() {
        let mut recorder = AnchorStabilityRecorder::new();
        recorder.record(AnchorFrame::from_rows(&["a".to_string()]));
        assert!(!recorder.record(AnchorFrame::from_rows(&["b".to_string()])));
        assert_eq!(recorder.stability(), 0.0);
    }

    #[test]
    fn wrapped_map_resolves_logical_rows() {
        let map = WrappedLineMap::build(&[2, 1, 3]);
        assert_eq!(map.logical_for_wrapped(0), Some(0));
        assert_eq!(map.logical_for_wrapped(1), Some(0));
        assert_eq!(map.logical_for_wrapped(2), Some(1));
        assert_eq!(map.logical_for_wrapped(3), Some(2));
        assert_eq!(map.logical_for_wrapped(9), None);
        assert_eq!(map.logical_len(), 3);
    }
}
