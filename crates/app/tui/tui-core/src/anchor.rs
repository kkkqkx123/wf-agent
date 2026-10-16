//! Anchor stability: visual regression signal for scroll and streaming.
//!
//! Cost metrics prove frames are cheap; stability proves they look steady.
//! The recorder hashes each frame with blank rows marked, so anchor drift
//! across scroll and streaming frames becomes a comparable number.
//!
//! This module owns no jump targets: session navigation anchors live in the
//! interactive controller, this recorder only hashes rendered frames.

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
    pub relocated_frames: u64,
    pub inserted_above_frames: u64,
    pub large_jump_frames: u64,
    pub rearranged_frames: u64,
}

/// Anchor shift class for one frame transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorShift {
    /// Content rows match in place.
    Stable,
    /// Same rows shifted by a small scroll offset.
    Relocated,
    /// New rows inserted above the anchor (prepend / upper splice).
    InsertedAbove,
    /// Viewport moved by more than half the visible height.
    LargeJump,
    /// Row order changed beyond a shift (reorder / full rearrangement).
    FullRearrangement,
}

/// Classify the shift from `prev` to `next` given the viewport height.
///
/// Pure function over row hashes: blanks are ignored, a common subsequence
/// offset means relocation, a shared suffix with a new head means insertion
/// above, an offset beyond half the viewport means a large jump, and a
/// changed order with shared rows means full rearrangement.
pub fn classify_shift(prev: &AnchorFrame, next: &AnchorFrame, viewport_h: usize) -> AnchorShift {
    let a: Vec<u64> = prev
        .row_hashes
        .iter()
        .copied()
        .filter(|h| *h != BLANK_ROW_HASH)
        .collect();
    let b: Vec<u64> = next
        .row_hashes
        .iter()
        .copied()
        .filter(|h| *h != BLANK_ROW_HASH)
        .collect();
    if a == b {
        return AnchorShift::Stable;
    }
    if a.is_empty() || b.is_empty() {
        return AnchorShift::FullRearrangement;
    }
    if let Some(offset) = shift_offset(&a, &b) {
        let half = viewport_h.max(1) / 2;
        if offset == 0 {
            // Same alignment but different content: tail append or edit.
            return AnchorShift::Relocated;
        }
        if offset > half {
            return AnchorShift::LargeJump;
        }
        // A pure suffix match with a new head is an upper insertion,
        // otherwise a small scroll relocation.
        if b.len() > a.len() && b[b.len() - a.len().min(b.len())..] == a[..a.len().min(b.len())] {
            return AnchorShift::InsertedAbove;
        }
        return AnchorShift::Relocated;
    }
    // Shared rows in a different order mean rearrangement.
    let shared = b.iter().any(|h| a.contains(h));
    if shared {
        return AnchorShift::FullRearrangement;
    }
    AnchorShift::LargeJump
}

/// Longest-alignment offset between two hash sequences, if they share a
/// contiguous run of at least half the shorter sequence.
fn shift_offset(a: &[u64], b: &[u64]) -> Option<usize> {
    let need = a.len().min(b.len()) / 2;
    if need == 0 {
        return None;
    }
    for offset in 0..=a.len().max(b.len()) {
        let overlap_a = longest_overlap(a, b, offset);
        if overlap_a >= need {
            return Some(offset);
        }
    }
    None
}

fn longest_overlap(a: &[u64], b: &[u64], offset: usize) -> usize {
    let mut best = 0usize;
    for start_a in 0..a.len() {
        let mut len = 0usize;
        while start_a + len < a.len() {
            let pos_b = start_a + len + offset;
            if pos_b >= b.len() || a[start_a + len] != b[pos_b] {
                break;
            }
            len += 1;
        }
        if len > best {
            best = len;
        }
        // Also try the mirrored direction (b shifted against a).
        let mut len = 0usize;
        while start_a + len < a.len() {
            if start_a + len < offset {
                break;
            }
            let pos_b = start_a + len - offset;
            if pos_b >= b.len() || a[start_a + len] != b[pos_b] {
                break;
            }
            len += 1;
        }
        if len > best {
            best = len;
        }
    }
    best
}

impl AnchorStabilityRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame, returning true when the anchor matches the previous.
    pub fn record(&mut self, frame: AnchorFrame) -> bool {
        let had_previous = self.previous.is_some();
        let (shift, _) = self.record_classified(frame, usize::MAX);
        had_previous && shift == AnchorShift::Stable
    }

    /// Record one frame with a viewport height, returning stability plus the
    /// shift class for the transition.
    pub fn record_classified(
        &mut self,
        frame: AnchorFrame,
        viewport_h: usize,
    ) -> (AnchorShift, bool) {
        let shift = self
            .previous
            .as_ref()
            .map_or(AnchorShift::Stable, |prev| classify_shift(prev, &frame, viewport_h));
        let stable = shift == AnchorShift::Stable;
        if self.previous.is_some() {
            if stable {
                self.stable_frames += 1;
            } else {
                self.unstable_frames += 1;
                match shift {
                    AnchorShift::Stable => {}
                    AnchorShift::Relocated => self.relocated_frames += 1,
                    AnchorShift::InsertedAbove => self.inserted_above_frames += 1,
                    AnchorShift::LargeJump => self.large_jump_frames += 1,
                    AnchorShift::FullRearrangement => self.rearranged_frames += 1,
                }
            }
        }
        self.previous = Some(frame);
        (shift, stable)
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

    fn rows(words: &[&str]) -> AnchorFrame {
        AnchorFrame::from_rows(
            &words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>(),
        )
    }

    #[test]
    fn shift_classes_cover_relocation_insertion_jump_and_rearrangement() {
        let base = rows(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        assert_eq!(classify_shift(&base, &base, 8), AnchorShift::Stable);
        let appended = rows(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
        assert_eq!(
            classify_shift(&base, &appended, 8),
            AnchorShift::Relocated
        );
        let prepended = rows(&["z", "a", "b", "c", "d", "e", "f", "g", "h"]);
        assert_eq!(
            classify_shift(&base, &prepended, 8),
            AnchorShift::InsertedAbove
        );
        let jumped = rows(&["m", "n", "o", "p", "q", "r", "s", "t"]);
        assert_eq!(classify_shift(&base, &jumped, 8), AnchorShift::LargeJump);
        let reordered = rows(&["a", "c", "b", "d", "f", "e", "g", "h"]);
        assert_eq!(
            classify_shift(&base, &reordered, 8),
            AnchorShift::FullRearrangement
        );
    }

    #[test]
    fn classified_recorder_counts_each_shift_kind() {
        let mut recorder = AnchorStabilityRecorder::new();
        recorder.record(rows(&["a", "b", "c", "d"]));
        let (shift, stable) = recorder.record_classified(rows(&["a", "b", "c", "d"]), 4);
        assert_eq!(shift, AnchorShift::Stable);
        assert!(stable);
        assert_eq!(recorder.stable_frames, 1);
        recorder.record_classified(rows(&["x", "a", "b", "c", "d"]), 4);
        assert_eq!(recorder.inserted_above_frames + recorder.relocated_frames, 1);
        assert!(recorder.stability() < 1.0);
    }
}
