//! Frame metrics and budget assertions for render optimization.
//!
//! [`FrameMetrics`] collects one [`FrameMetric`] per frame: changed cells,
//! diff bytes, wall time, actually laid-out rows and parsed bytes, plus the
//! perceptual signals from later parity stages: flicker cells, key-to-paint
//! latency, preparation cache hits/misses and per-stage timings. The
//! production draw path stays allocation-free on the hot path (callers fill
//! the extended fields they know, the rest default to zero); tests, headless
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
    /// Cells repainted without a content cause (full clears, placeholder
    /// flashes): the flicker numerator beside `changed_cells`.
    pub flicker_cells: usize,
    /// Key-to-paint latency in milliseconds (input arrival to frame done).
    pub key_to_paint_ms: u64,
    /// Preparation cache hits observed while composing this frame.
    pub cache_hits: usize,
    /// Preparation cache misses observed while composing this frame.
    pub cache_misses: usize,
    /// Layout stage wall time in milliseconds.
    pub stage_layout_ms: u64,
    /// Highlight stage wall time in milliseconds.
    pub stage_highlight_ms: u64,
    /// Compose and post-process stage wall time in milliseconds.
    pub stage_compose_ms: u64,
}

impl FrameMetric {
    /// Build a metric from its raw signals (extended fields default to zero
    /// so existing call sites keep compiling).
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
            ..Self::default()
        }
    }

    /// Attach flicker cells to a metric.
    pub fn with_flicker(mut self, flicker_cells: usize) -> Self {
        self.flicker_cells = flicker_cells;
        self
    }

    /// Attach key-to-paint latency to a metric.
    pub fn with_key_to_paint(mut self, key_to_paint_ms: u64) -> Self {
        self.key_to_paint_ms = key_to_paint_ms;
        self
    }

    /// Attach preparation cache hits and misses to a metric.
    pub fn with_cache(mut self, hits: usize, misses: usize) -> Self {
        self.cache_hits = hits;
        self.cache_misses = misses;
        self
    }

    /// Attach per-stage timings to a metric.
    pub fn with_stages(mut self, layout_ms: u64, highlight_ms: u64, compose_ms: u64) -> Self {
        self.stage_layout_ms = layout_ms;
        self.stage_highlight_ms = highlight_ms;
        self.stage_compose_ms = compose_ms;
        self
    }

    /// Flicker rate: flicker cells over changed cells (0 when nothing changed).
    pub fn flicker_rate(&self) -> f64 {
        if self.changed_cells == 0 {
            return 0.0;
        }
        self.flicker_cells as f64 / self.changed_cells as f64
    }

    /// Preparation cache hit rate for this frame (1.0 when nothing was probed).
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 1.0;
        }
        self.cache_hits as f64 / total as f64
    }
}

/// Flicker detector: counts cells repainted without a content cause.
///
/// A full-surface clear or a placeholder flash repaints rows whose content
/// hash did not change. Callers feed the changed-cell count plus the count
/// of changed cells with an actual content difference; the gap is flicker.
#[derive(Debug, Default)]
pub struct FlickerDetector {
    total_flicker: u64,
    total_changed: u64,
    flicker_frames: u64,
    frames: u64,
}

impl FlickerDetector {
    /// Empty detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame: `changed` cells repainted, `content_changed` of them
    /// backed by a real content difference.
    pub fn record(&mut self, changed: usize, content_changed: usize) {
        let flicker = changed.saturating_sub(content_changed);
        self.total_flicker += flicker as u64;
        self.total_changed += changed as u64;
        self.frames += 1;
        if flicker > 0 {
            self.flicker_frames += 1;
        }
    }

    /// Overall flicker rate across recorded frames.
    pub fn flicker_rate(&self) -> f64 {
        if self.total_changed == 0 {
            return 0.0;
        }
        self.total_flicker as f64 / self.total_changed as f64
    }

    /// Share of frames containing any flicker.
    pub fn flicker_frame_share(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.flicker_frames as f64 / self.frames as f64
    }
}

/// Key-to-paint tracker: input arrival to frame completion latency.
///
/// The event loop stamps the newest input time; the submit point stamps the
/// paint completion. The tracker keeps the worst and running mean so budgets
/// can assert both tail and steady-state latency.
#[derive(Debug, Default)]
pub struct KeyPaintTracker {
    pending_input_ms: Option<u64>,
    worst_ms: u64,
    total_ms: u64,
    samples: u64,
}

impl KeyPaintTracker {
    /// Empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Note an input arrival at `now_ms`.
    pub fn note_input(&mut self, now_ms: u64) {
        self.pending_input_ms = Some(now_ms);
    }

    /// Note a painted frame at `now_ms`, returning the sample latency when an
    /// input was pending.
    pub fn note_paint(&mut self, now_ms: u64) -> Option<u64> {
        let start = self.pending_input_ms.take()?;
        let latency = now_ms.saturating_sub(start);
        self.samples += 1;
        self.total_ms += latency;
        if latency > self.worst_ms {
            self.worst_ms = latency;
        }
        Some(latency)
    }

    /// Worst observed key-to-paint latency, if any input was painted.
    pub fn worst_ms(&self) -> Option<u64> {
        if self.samples == 0 {
            None
        } else {
            Some(self.worst_ms)
        }
    }

    /// Mean key-to-paint latency, if any input was painted.
    pub fn mean_ms(&self) -> Option<f64> {
        if self.samples == 0 {
            None
        } else {
            Some(self.total_ms as f64 / self.samples as f64)
        }
    }
}

/// Per-stage timing split for one frame composition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTiming {
    /// Preparation and layout wall time in milliseconds.
    pub layout_ms: u64,
    /// Syntax highlight upgrade wall time in milliseconds.
    pub highlight_ms: u64,
    /// Composition and post-process wall time in milliseconds.
    pub compose_ms: u64,
}

impl StageTiming {
    /// Build a timing split.
    pub fn new(layout_ms: u64, highlight_ms: u64, compose_ms: u64) -> Self {
        Self {
            layout_ms,
            highlight_ms,
            compose_ms,
        }
    }

    /// Total staged wall time.
    pub fn total_ms(&self) -> u64 {
        self.layout_ms + self.highlight_ms + self.compose_ms
    }
}

/// Preparation cache hit-rate accumulator across frames.
#[derive(Debug, Default)]
pub struct PrepHitRate {
    hits: u64,
    misses: u64,
}

impl PrepHitRate {
    /// Empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame's probe counts.
    pub fn record(&mut self, hits: usize, misses: usize) {
        self.hits += hits as u64;
        self.misses += misses as u64;
    }

    /// Overall hit rate (1.0 when nothing was probed).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 1.0;
        }
        self.hits as f64 / total as f64
    }
}

/// Steady-state budget thresholds for regression assertions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBudgets {
    /// Maximum laid-out rows per steady frame (after the first).
    pub max_laid_out_rows: usize,
    /// Maximum parsed bytes per steady frame (after the first).
    pub max_parsed_bytes: usize,
    /// Maximum changed cells per steady frame (after the first).
    pub max_changed_cells: usize,
    /// Maximum flicker rate per steady frame.
    pub max_flicker_rate: f64,
    /// Maximum key-to-paint latency in milliseconds.
    pub max_key_to_paint_ms: u64,
    /// Maximum frame wall time in milliseconds.
    pub max_elapsed_ms: u64,
}

impl Default for FrameBudgets {
    fn default() -> Self {
        Self {
            max_laid_out_rows: 4,
            max_parsed_bytes: 512,
            max_changed_cells: 800,
            max_flicker_rate: 0.05,
            max_key_to_paint_ms: 150,
            max_elapsed_ms: 50,
        }
    }
}

/// Regression report: pass/fail verdict plus the human-readable detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegressionReport {
    /// True when every steady frame met every budget.
    pub passed: bool,
    /// One line per violated budget (empty when passing).
    pub violations: Vec<String>,
    /// Full metrics summary for logs.
    pub summary: String,
}

impl RegressionReport {
    /// Whether the run met all budgets.
    pub fn passed(&self) -> bool {
        self.passed
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

    /// Total flicker cells across all frames.
    pub fn total_flicker_cells(&self) -> usize {
        self.frames.iter().map(|f| f.flicker_cells).sum()
    }

    /// Worst frame wall time in milliseconds, if any frame was recorded.
    pub fn worst_elapsed_ms(&self) -> Option<u64> {
        self.frames.iter().map(|f| f.elapsed_ms).max()
    }

    /// Worst key-to-paint latency in milliseconds, if any frame recorded one.
    pub fn worst_key_to_paint_ms(&self) -> Option<u64> {
        self.frames.iter().map(|f| f.key_to_paint_ms).max()
    }

    /// Mean preparation hit rate across frames that probed the cache.
    pub fn mean_hit_rate(&self) -> f64 {
        let mut hits: u64 = 0;
        let mut total: u64 = 0;
        for frame in &self.frames {
            hits += frame.cache_hits as u64;
            total += (frame.cache_hits + frame.cache_misses) as u64;
        }
        if total == 0 {
            return 1.0;
        }
        hits as f64 / total as f64
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

    /// Budget check: every steady frame parses at most `max_bytes`.
    pub fn steady_parse_within(&self, max_bytes: usize) -> bool {
        self.frames
            .iter()
            .skip(1)
            .all(|f| f.parsed_bytes <= max_bytes)
    }

    /// Budget check: every steady frame changes at most `max_cells`.
    pub fn steady_changed_within(&self, max_cells: usize) -> bool {
        self.frames
            .iter()
            .skip(1)
            .all(|f| f.changed_cells <= max_cells)
    }

    /// Budget check: every steady frame stays under `max_rate` flicker.
    pub fn steady_flicker_within(&self, max_rate: f64) -> bool {
        self.frames
            .iter()
            .skip(1)
            .all(|f| f.flicker_rate() <= max_rate)
    }

    /// Budget check: every painted key lands within `max_ms`.
    pub fn key_to_paint_within(&self, max_ms: u64) -> bool {
        self.frames
            .iter()
            .filter(|f| f.key_to_paint_ms > 0)
            .all(|f| f.key_to_paint_ms <= max_ms)
    }

    /// One-line baseline report: frame count, changed cells, diff bytes,
    /// laid-out rows, parsed bytes, flicker cells, worst frame time and
    /// worst key-to-paint latency.
    pub fn report(&self) -> String {
        format!(
            "frames={} changed_cells={} diff_bytes={} laid_out_rows={} parsed_bytes={} flicker_cells={} worst_ms={} worst_key_paint_ms={} hit_rate={:.3}",
            self.len(),
            self.total_changed_cells(),
            self.total_diff_bytes(),
            self.total_laid_out_rows(),
            self.total_parsed_bytes(),
            self.total_flicker_cells(),
            self.worst_elapsed_ms().unwrap_or(0),
            self.worst_key_to_paint_ms().unwrap_or(0),
            self.mean_hit_rate(),
        )
    }

    /// Check steady-state budgets frame by frame, returning a fail-fast
    /// regression report. The first frame is the cold baseline and is never
    /// budgeted; every later frame must meet all thresholds.
    pub fn check_regression(&self, budgets: &FrameBudgets) -> RegressionReport {
        let mut violations = Vec::new();
        for frame in self.frames.iter().skip(1) {
            if frame.laid_out_rows > budgets.max_laid_out_rows {
                violations.push(format!(
                    "frame {} laid_out_rows={} over {}",
                    frame.frame, frame.laid_out_rows, budgets.max_laid_out_rows
                ));
            }
            if frame.parsed_bytes > budgets.max_parsed_bytes {
                violations.push(format!(
                    "frame {} parsed_bytes={} over {}",
                    frame.frame, frame.parsed_bytes, budgets.max_parsed_bytes
                ));
            }
            if frame.changed_cells > budgets.max_changed_cells {
                violations.push(format!(
                    "frame {} changed_cells={} over {}",
                    frame.frame, frame.changed_cells, budgets.max_changed_cells
                ));
            }
            if frame.flicker_rate() > budgets.max_flicker_rate {
                violations.push(format!(
                    "frame {} flicker_rate={:.3} over {:.3}",
                    frame.frame,
                    frame.flicker_rate(),
                    budgets.max_flicker_rate
                ));
            }
            if frame.key_to_paint_ms > budgets.max_key_to_paint_ms {
                violations.push(format!(
                    "frame {} key_to_paint_ms={} over {}",
                    frame.frame, frame.key_to_paint_ms, budgets.max_key_to_paint_ms
                ));
            }
            if frame.elapsed_ms > budgets.max_elapsed_ms {
                violations.push(format!(
                    "frame {} elapsed_ms={} over {}",
                    frame.frame, frame.elapsed_ms, budgets.max_elapsed_ms
                ));
            }
        }
        let passed = violations.is_empty();
        let summary = format!(
            "{} | {}",
            self.report(),
            if passed { "PASS" } else { "FAIL" }
        );
        RegressionReport {
            passed,
            violations,
            summary,
        }
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

    #[test]
    fn flicker_detector_counts_unexplained_repaints() {
        let mut detector = FlickerDetector::new();
        detector.record(10, 10);
        assert_eq!(detector.flicker_rate(), 0.0);
        detector.record(10, 2);
        assert!(detector.flicker_rate() > 0.0);
        assert!(detector.flicker_frame_share() > 0.0);
    }

    #[test]
    fn key_paint_tracker_reports_tail_latency() {
        let mut tracker = KeyPaintTracker::new();
        assert_eq!(tracker.worst_ms(), None);
        tracker.note_input(100);
        assert_eq!(tracker.note_paint(140), Some(40));
        assert_eq!(tracker.worst_ms(), Some(40));
        assert!(tracker.mean_ms().is_some_and(|mean| mean == 40.0));
    }

    #[test]
    fn steady_budgets_cover_parse_change_flicker_and_latency() {
        let mut metrics = FrameMetrics::new();
        metrics.record(metric(0, 50));
        metrics.record(
            FrameMetric::new(1, 10, 80, 2, 2, 40)
                .with_flicker(0)
                .with_key_to_paint(30)
                .with_cache(9, 1),
        );
        assert!(metrics.steady_parse_within(40));
        assert!(metrics.steady_changed_within(10));
        assert!(metrics.steady_flicker_within(0.05));
        assert!(metrics.key_to_paint_within(150));
        assert!((metrics.mean_hit_rate() - 0.9).abs() < 1e-9);
    }

    #[test]
    fn regression_report_fails_on_degraded_frames() {
        let mut metrics = FrameMetrics::new();
        metrics.record(metric(0, 50));
        metrics.record(metric(1, 2).with_flicker(0).with_key_to_paint(10));
        let ok = metrics.check_regression(&FrameBudgets::default());
        assert!(ok.passed());
        assert!(ok.summary.contains("PASS"));
        metrics.record(
            FrameMetric::new(2, 700, 5_000, 80, 60, 4_000)
                .with_flicker(600)
                .with_key_to_paint(900),
        );
        let bad = metrics.check_regression(&FrameBudgets::default());
        assert!(!bad.passed());
        assert!(!bad.violations.is_empty());
        assert!(bad.summary.contains("FAIL"));
    }
}
