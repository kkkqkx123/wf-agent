//! Frame metrics and budget assertions for render optimization.
//!
//! [`FrameMetrics`] collects one [`FrameMetric`] per frame: changed cells,
//! diff bytes, wall time, actually laid-out rows and parsed bytes. The
//! production draw path stays untouched (zero overhead); tests, headless
//! renders and the `diff-record` backend feed this collector and assert
//! budgets on it, e.g. steady streaming frames lay out only the new rows.

/// One frame's worth of render cost signals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameMetric {
    /// Frame sequence number.
    pub frame: u64,
    /// Cells whose symbol or style differs from the previous frame.
    pub changed_cells: usize,
    /// Approximate diff payload in bytes.
    pub diff_bytes: usize,
    /// Frame wall time in milliseconds.
    pub elapsed_ms: u64,
    /// Source rows actually laid out for this frame.
    pub laid_out_rows: usize,
    /// Source bytes parsed for this frame.
    pub parsed_bytes: usize,
}

impl FrameMetric {
    /// Build a metric from its raw signals.
    pub fn new(
        frame: u64,
        changed_cells: usize,
        diff_bytes: usize,
        elapsed_ms: u64,
        laid_out_rows: usize,
        parsed_bytes: usize,
    ) -> Self {
        Self {
            frame,
            changed_cells,
            diff_bytes,
            elapsed_ms,
            laid_out_rows,
            parsed_bytes,
        }
    }
}

/// Collector producing baseline reports and budget verdicts.
#[derive(Debug, Default)]
pub struct FrameMetrics {
    frames: Vec<FrameMetric>,
}

impl FrameMetrics {
    /// Empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame.
    pub fn record(&mut self, metric: FrameMetric) {
        self.frames.push(metric);
    }

    /// Recorded frames in order.
    pub fn frames(&self) -> &[FrameMetric] {
        &self.frames
    }

    /// Number of recorded frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether any frame was recorded.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Total changed cells across all frames.
    pub fn total_changed_cells(&self) -> usize {
        self.frames.iter().map(|f| f.changed_cells).sum()
    }

    /// Total diff bytes across all frames.
    pub fn total_diff_bytes(&self) -> usize {
        self.frames.iter().map(|f| f.diff_bytes).sum()
    }

    /// Total laid-out rows across all frames.
    pub fn total_laid_out_rows(&self) -> usize {
        self.frames.iter().map(|f| f.laid_out_rows).sum()
    }

    /// Total parsed bytes across all frames.
    pub fn total_parsed_bytes(&self) -> usize {
        self.frames.iter().map(|f| f.parsed_bytes).sum()
    }

    /// Worst frame wall time in milliseconds, if any frame was recorded.
    pub fn worst_elapsed_ms(&self) -> Option<u64> {
        self.frames.iter().map(|f| f.elapsed_ms).max()
    }

    /// Budget check: every frame after the first lays out at most
    /// `max_rows_per_frame` rows. Steady streaming passes with the new-row
    /// count as the bound.
    pub fn steady_layout_within(&self, max_rows_per_frame: usize) -> bool {
        self.frames
            .iter()
            .skip(1)
            .all(|f| f.laid_out_rows <= max_rows_per_frame)
    }

    /// Budget check: no frame exceeds `max_elapsed_ms` wall time.
    pub fn all_frames_within(&self, max_elapsed_ms: u64) -> bool {
        self.frames.iter().all(|f| f.elapsed_ms <= max_elapsed_ms)
    }

    /// One-line baseline report: frame count, changed cells, diff bytes,
    /// laid-out rows, parsed bytes and worst frame time.
    pub fn report(&self) -> String {
        format!(
            "frames={} changed_cells={} diff_bytes={} laid_out_rows={} parsed_bytes={} worst_ms={}",
            self.len(),
            self.total_changed_cells(),
            self.total_diff_bytes(),
            self.total_laid_out_rows(),
            self.total_parsed_bytes(),
            self.worst_elapsed_ms().unwrap_or(0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(frame: u64, rows: usize) -> FrameMetric {
        FrameMetric::new(frame, 12, 96, 2, rows, 40)
    }

    #[test]
    fn collector_totals_and_report() {
        let mut metrics = FrameMetrics::new();
        metrics.record(metric(0, 50));
        metrics.record(metric(1, 2));
        metrics.record(metric(2, 2));
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics.total_changed_cells(), 36);
        assert_eq!(metrics.total_laid_out_rows(), 54);
        assert_eq!(metrics.total_parsed_bytes(), 120);
        assert_eq!(metrics.worst_elapsed_ms(), Some(2));
        let report = metrics.report();
        assert!(report.contains("frames=3"));
        assert!(report.contains("laid_out_rows=54"));
    }

    #[test]
    fn steady_layout_budget_flags_full_relayouts() {
        let mut metrics = FrameMetrics::new();
        metrics.record(metric(0, 50));
        metrics.record(metric(1, 2));
        metrics.record(metric(2, 2));
        assert!(metrics.steady_layout_within(2));
        assert!(!metrics.steady_layout_within(1));
        metrics.record(metric(3, 50));
        assert!(!metrics.steady_layout_within(2));
    }

    #[test]
    fn frame_time_budget() {
        let mut metrics = FrameMetrics::new();
        metrics.record(metric(0, 1));
        assert!(metrics.all_frames_within(2));
        assert!(!metrics.all_frames_within(1));
    }
}
