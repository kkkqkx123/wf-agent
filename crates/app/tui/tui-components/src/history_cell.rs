//! Transcript history cell model and virtualization helpers.
//!
//! The full-TUI transcript is moving from line-oriented rendering to
//! cell-oriented rendering. A [`HistoryCell`] is a reflowable transcript
//! entry (raw source text plus an optional display rendering), and the
//! helpers here provide deterministic virtualized windows over a cell list.

use crate::transcript::HistoryLine;

/// Alias for a transcript cell. Cells reuse the existing
/// [`HistoryLine`] reflow implementation so rendering stays consistent
/// during the migration.
pub type HistoryCell = HistoryLine;

/// Render mode for a history cell.
pub use crate::transcript::LineRender as HistoryRenderMode;

/// Stable identifier used by cell-based copy and virtualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub u64);

/// One (maybe empty) page of cells selected for the current viewport.
pub fn virtualized_cells(cells: &[HistoryCell], start: usize, height: usize) -> &[HistoryCell] {
    if height == 0 {
        return &[];
    }
    let start = start.min(cells.len());
    let end = start.saturating_add(height).min(cells.len());
    &cells[start..end]
}

/// Total number of cells after a window offset is applied.
pub fn cell_count_after_offset(cells: &[HistoryCell], offset: usize) -> usize {
    cells.len().saturating_sub(offset)
}

/// Slice a list of cells into contiguous chunks for incremental reflow.
pub fn chunk_cells(cells: &[HistoryCell], chunk_size: usize) -> Vec<Vec<HistoryCell>> {
    if chunk_size == 0 {
        return vec![cells.to_vec()];
    }
    cells
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtualized_window_clamps_bounds() {
        let cells = vec![
            HistoryCell::new("a"),
            HistoryCell::new("b"),
            HistoryCell::new("c"),
        ];
        let window = virtualized_cells(&cells, 1, 10);
        assert_eq!(window.len(), 2);
        assert_eq!(virtualized_cells(&cells, 10, 2).len(), 0);
    }

    #[test]
    fn chunking_is_deterministic() {
        let cells = vec![
            HistoryCell::new("a"),
            HistoryCell::new("b"),
            HistoryCell::new("c"),
            HistoryCell::new("d"),
        ];
        let chunks = chunk_cells(&cells, 2);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 2);
        assert_eq!(chunks[1].len(), 2);
    }

    #[test]
    fn mode_alias_round_trips() {
        let cell = HistoryCell::markdown("hello");
        assert_eq!(cell.role, crate::transcript::Role::Default);
        let _ = HistoryRenderMode::Markdown;
    }
}
