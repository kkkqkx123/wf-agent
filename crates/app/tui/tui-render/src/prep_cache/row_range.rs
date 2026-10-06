//! Display-row interval belonging to one source line.

/// Display-row interval belonging to one source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRange {
    /// Start offset inside the flat display-row array.
    pub start: usize,
    /// Number of display rows for this source line.
    pub len: usize,
}
