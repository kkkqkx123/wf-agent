//! Observability closure: draw attribution, slow frames, flicker history,
//! key-to-paint distribution, anchor rates, replay coverage and atlas bounds.
//!
//! The production draw path stays allocation-free on the hot path: callers
//! fill only the fields they know and the rest default to zero. Collection
//! lives behind these explicit histories; debug and demo entries drive them,
//! never the release frame loop.

use std::collections::{HashSet, VecDeque};

use crate::anchor::{classify_shift, AnchorFrame, AnchorShift};
use crate::render_model::RecordedEvent;

/// Maximum draw-call samples retained for attribution.
pub const DRAW_CALL_HISTORY_MAX_SAMPLES: usize = 240;
/// Maximum slow-frame samples retained.
pub const SLOW_FRAME_HISTORY_MAX_SAMPLES: usize = 128;
/// Maximum flicker-frame samples retained.
pub const FLICKER_HISTORY_MAX_SAMPLES: usize = 256;
/// Maximum flicker events retained.
pub const FLICKER_EVENTS_MAX_SAMPLES: usize = 128;
/// Maximum key-to-paint samples retained.
pub const KEY_TO_PAINT_MAX_SAMPLES: usize = 64;
/// Maximum anchor jarring events retained.
pub const ANCHOR_EVENTS_MAX_SAMPLES: usize = 64;

/// Quantile over sorted `values` at probability `p` in `[0, 1]`.
pub fn percentile_u64(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let clamped = p.clamp(0.0, 1.0);
    let rank = ((sorted.len() as f64 * clamped).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn sorted_clone(values: &VecDeque<u64>) -> Vec<u64> {
    let mut out: Vec<u64> = values.iter().copied().collect();
    out.sort_unstable();
    out
}

/// Per-draw input cause: what the user did before this draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameInputAttribution {
    /// Stable event label (`key:enter`, `resize`, `tick`, empty when unknown).
    pub event: String,
    /// Scroll delta applied before this draw.
    pub scroll_delta: i32,
    /// Whether the model picker overlay was open.
    pub model_picker_open: bool,
}

impl FrameInputAttribution {
    /// Empty attribution for production frames without input context.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Attribution for a key label.
    pub fn for_key(label: &str) -> Self {
        Self {
            event: format!("key:{label}"),
            scroll_delta: 0,
            model_picker_open: false,
        }
    }
}

/// One draw-call attribution sample.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrawCallAttribution {
    /// Caller-supplied clock for the draw.
    pub timestamp_ms: u64,
    /// Total draw wall time in milliseconds.
    pub total_ms: u64,
    /// Renderer composition time in milliseconds.
    pub render_ms: u64,
    /// Backend flush time in milliseconds.
    pub backend_flush_ms: u64,
    /// Cells changed by the diff.
    pub changed_cells: usize,
    /// Total cells on the surface.
    pub total_cells: usize,
    /// Whether the frame forced a full redraw.
    pub force_full_redraw: bool,
    /// Input cause behind this draw.
    pub input: FrameInputAttribution,
}

impl DrawCallAttribution {
    /// Changed-cell ratio in `[0, 1]`.
    pub fn changed_ratio(&self) -> f64 {
        if self.total_cells == 0 {
            return 0.0;
        }
        self.changed_cells as f64 / self.total_cells as f64
    }
}

/// Ring of draw-call attributions with distribution summaries.
#[derive(Debug, Clone, Default)]
pub struct DrawCallHistory {
    samples: VecDeque<DrawCallAttribution>,
}

impl DrawCallHistory {
    /// Empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one draw; evicts the oldest beyond the bound.
    pub fn record(&mut self, sample: DrawCallAttribution) {
        self.samples.push_back(sample);
        while self.samples.len() > DRAW_CALL_HISTORY_MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Retained sample count.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no draw was recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Clear retained samples.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    fn totals(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self.samples.iter().map(|s| s.total_ms).collect();
        out.sort_unstable();
        out
    }

    fn renders(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self.samples.iter().map(|s| s.render_ms).collect();
        out.sort_unstable();
        out
    }

    /// Mean total wall time, if any draw was recorded.
    pub fn mean_total_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let total: u64 = self.samples.iter().map(|s| s.total_ms).sum();
        Some(total as f64 / self.samples.len() as f64)
    }

    /// Median total wall time.
    pub fn p50_total_ms(&self) -> u64 {
        percentile_u64(&self.totals(), 0.5)
    }

    /// 95th percentile total wall time.
    pub fn p95_total_ms(&self) -> u64 {
        percentile_u64(&self.totals(), 0.95)
    }

    /// Maximum total wall time.
    pub fn max_total_ms(&self) -> u64 {
        self.totals().last().copied().unwrap_or(0)
    }

    /// Mean render time.
    pub fn mean_render_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let total: u64 = self.samples.iter().map(|s| s.render_ms).sum();
        Some(total as f64 / self.samples.len() as f64)
    }

    /// 95th percentile render time.
    pub fn p95_render_ms(&self) -> u64 {
        percentile_u64(&self.renders(), 0.95)
    }

    /// Maximum render time.
    pub fn max_render_ms(&self) -> u64 {
        self.renders().last().copied().unwrap_or(0)
    }

    /// Mean changed-cell ratio across retained draws.
    pub fn mean_changed_ratio(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let total: f64 = self.samples.iter().map(|s| s.changed_ratio()).sum();
        total / self.samples.len() as f64
    }

    /// Draws per second over `window_ms` of retained timestamps.
    pub fn draws_per_second(&self, window_ms: u64) -> f64 {
        if self.samples.is_empty() || window_ms == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (window_ms as f64 / 1000.0)
    }

    /// Debug JSON with distribution summaries for the newest `limit` draws.
    pub fn debug_json(&self, limit: usize) -> String {
        let take = limit.min(self.samples.len());
        let start = self.samples.len().saturating_sub(take);
        let mut items = Vec::new();
        for sample in self.samples.iter().skip(start) {
            items.push(format!(
                "{{\"ts\":{},\"total\":{},\"render\":{},\"flush\":{},\"changed\":{},\"total_cells\":{},\"full\":{},\"event\":\"{}\"}}",
                sample.timestamp_ms,
                sample.total_ms,
                sample.render_ms,
                sample.backend_flush_ms,
                sample.changed_cells,
                sample.total_cells,
                sample.force_full_redraw,
                sample.input.event.replace('"', "'"),
            ));
        }
        format!(
            "{{\"samples\":[{}],\"count\":{},\"mean_total\":{:.2},\"p50_total\":{},\"p95_total\":{},\"max_total\":{},\"mean_render\":{:.2},\"p95_render\":{},\"max_render\":{},\"changed_ratio\":{:.3}}}",
            items.join(","),
            self.samples.len(),
            self.mean_total_ms().unwrap_or(0.0),
            self.p50_total_ms(),
            self.p95_total_ms(),
            self.max_total_ms(),
            self.mean_render_ms().unwrap_or(0.0),
            self.p95_render_ms(),
            self.max_render_ms(),
            self.mean_changed_ratio(),
        )
    }
}

/// Per-frame host resource attribution. Production fills only the fields the
/// caller knows; the rest stay zero so the hot path never probes the host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameResourceAttribution {
    /// Frame wall time in milliseconds.
    pub wall_ms: u64,
    /// Process CPU time in milliseconds, when sampled.
    pub process_cpu_ms: u64,
    /// Process CPU ratio over the frame, when sampled.
    pub cpu_ratio: f64,
    /// Resident set size in MiB, when sampled.
    pub rss_mb: u64,
    /// Host one-minute load average, when sampled.
    pub host_load_1m: f64,
    /// Load per CPU, when sampled.
    pub host_load_per_cpu: f64,
    /// Host CPU count, when known.
    pub cpu_count: u64,
    /// Available memory in MiB, when known.
    pub mem_avail_mb: u64,
    /// Total memory in MiB, when known.
    pub mem_total_mb: u64,
    /// Pressure label (`idle`, `busy`, empty when unknown).
    pub pressure: String,
}

impl FrameResourceAttribution {
    /// Attribution carrying only wall time; every other field stays zero.
    pub fn wall_only(wall_ms: u64) -> Self {
        Self {
            wall_ms,
            ..Self::default()
        }
    }

    /// Best-effort host sample that never fails: wall time plus load average
    /// and CPU count when the platform exposes them.
    pub fn sample(wall_ms: u64) -> Self {
        let load = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|raw| {
                raw.split_whitespace()
                    .next()
                    .and_then(|head| head.parse::<f64>().ok())
            });
        let cpus = std::thread::available_parallelism()
            .ok()
            .map(|n| n.get() as u64);
        let (load_1m, per_cpu) = match (load, cpus) {
            (Some(value), Some(count)) if count > 0 => (value, value / count as f64),
            (Some(value), _) => (value, 0.0),
            _ => (0.0, 0.0),
        };
        let pressure = if per_cpu > 2.0 {
            "busy".to_string()
        } else if per_cpu > 0.0 {
            "idle".to_string()
        } else {
            String::new()
        };
        Self {
            wall_ms,
            host_load_1m: load_1m,
            host_load_per_cpu: per_cpu,
            cpu_count: cpus.unwrap_or(0),
            pressure,
            ..Self::default()
        }
    }
}

/// One slow-frame sample with prepare/draw/total split plus attribution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlowFrameSample {
    /// Caller-supplied clock for the frame.
    pub timestamp_ms: u64,
    /// Threshold in milliseconds that marked this frame slow.
    pub threshold_ms: u64,
    /// Preparation and layout time in milliseconds.
    pub prepare_ms: u64,
    /// Draw time in milliseconds.
    pub draw_ms: u64,
    /// Total frame time in milliseconds.
    pub total_ms: u64,
    /// Resource attribution for the frame.
    pub resource: FrameResourceAttribution,
    /// Input cause behind the slow frame.
    pub input: FrameInputAttribution,
    /// Scroll delta applied before the frame.
    pub scroll_delta: i32,
}

/// Environment override for the slow-frame threshold in milliseconds.
pub const SLOW_FRAME_THRESHOLD_ENV: &str = "WF_TUI_SLOW_FRAME_MS";
/// Default slow-frame threshold in milliseconds.
pub const SLOW_FRAME_THRESHOLD_DEFAULT_MS: u64 = 40;

/// Slow-frame threshold from the environment, defaulting to 40ms.
pub fn slow_frame_threshold_ms() -> u64 {
    std::env::var(SLOW_FRAME_THRESHOLD_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(SLOW_FRAME_THRESHOLD_DEFAULT_MS)
}

/// Ring of slow-frame samples with distribution summaries.
#[derive(Debug, Clone, Default)]
pub struct SlowFrameHistory {
    samples: VecDeque<SlowFrameSample>,
    last_log_at: Option<u64>,
}

impl SlowFrameHistory {
    /// Empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one slow frame; evicts the oldest beyond the bound.
    pub fn record(&mut self, sample: SlowFrameSample) {
        self.last_log_at = Some(sample.timestamp_ms);
        self.samples.push_back(sample);
        while self.samples.len() > SLOW_FRAME_HISTORY_MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Retained sample count.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no slow frame was recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Clear retained samples.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.last_log_at = None;
    }

    /// Maximum total wall time across retained samples.
    pub fn max_total_ms(&self) -> u64 {
        self.samples.iter().map(|s| s.total_ms).max().unwrap_or(0)
    }

    /// Maximum prepare time across retained samples.
    pub fn max_prepare_ms(&self) -> u64 {
        self.samples.iter().map(|s| s.prepare_ms).max().unwrap_or(0)
    }

    /// Maximum draw time across retained samples.
    pub fn max_draw_ms(&self) -> u64 {
        self.samples.iter().map(|s| s.draw_ms).max().unwrap_or(0)
    }

    /// Debug JSON with per-frame splits plus resource attribution.
    pub fn debug_json(&self) -> String {
        let mut items = Vec::new();
        for sample in &self.samples {
            items.push(format!(
                "{{\"ts\":{},\"threshold\":{},\"prepare\":{},\"draw\":{},\"total\":{},\"wall\":{},\"rss\":{},\"load\":{:.2},\"event\":\"{}\"}}",
                sample.timestamp_ms,
                sample.threshold_ms,
                sample.prepare_ms,
                sample.draw_ms,
                sample.total_ms,
                sample.resource.wall_ms,
                sample.resource.rss_mb,
                sample.resource.host_load_1m,
                sample.input.event.replace('"', "'"),
            ));
        }
        format!(
            "{{\"count\":{},\"max_total\":{},\"max_prepare\":{},\"max_draw\":{},\"samples\":[{}]}}",
            self.samples.len(),
            self.max_total_ms(),
            self.max_prepare_ms(),
            self.max_draw_ms(),
            items.join(","),
        )
    }
}

/// One flicker-frame sample: repainted without a content cause.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlickerFrameSample {
    /// Caller-supplied clock for the frame.
    pub timestamp_ms: u64,
    /// Visible content hash for the frame.
    pub visible_hash: u64,
    /// Streaming prefix hash for the frame.
    pub streaming_hash: u64,
    /// Batch hash for the frame.
    pub batch_hash: u64,
    /// Geometry label (`cols`x`rows`) for the frame.
    pub geometry: String,
    /// Preparation time in milliseconds.
    pub prepare_ms: u64,
    /// Draw time in milliseconds.
    pub draw_ms: u64,
    /// Total frame time in milliseconds.
    pub total_ms: u64,
}

/// One flicker transition event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlickerEvent {
    /// Caller-supplied clock for the event.
    pub timestamp_ms: u64,
    /// Event kind (`flash`, `clear`, `placeholder`).
    pub kind: String,
    /// Previous content hash.
    pub prev: u64,
    /// Current content hash.
    pub current: u64,
}

/// Interface hint surfaced when flicker recurs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlickerUiNotice {
    /// One-line summary for the status line.
    pub summary: String,
    /// Follow-up hint with log and debug pointers.
    pub hint: String,
}

/// Ring of flicker samples plus transition events and interface hints.
#[derive(Debug, Clone, Default)]
pub struct FlickerFrameHistory {
    samples: VecDeque<FlickerFrameSample>,
    events: VecDeque<FlickerEvent>,
}

impl FlickerFrameHistory {
    /// Empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one flicker-frame sample.
    pub fn record_sample(&mut self, sample: FlickerFrameSample) {
        self.samples.push_back(sample);
        while self.samples.len() > FLICKER_HISTORY_MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Record one flicker transition event.
    pub fn record_event(&mut self, event: FlickerEvent) {
        self.events.push_back(event);
        while self.events.len() > FLICKER_EVENTS_MAX_SAMPLES {
            self.events.pop_front();
        }
    }

    /// Retained sample count.
    pub fn sample_len(&self) -> usize {
        self.samples.len()
    }

    /// Retained event count.
    pub fn event_len(&self) -> usize {
        self.events.len()
    }

    /// Clear samples and events.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.events.clear();
    }

    /// Debug JSON with samples plus events.
    pub fn debug_json(&self, limit: usize) -> String {
        let take = limit.min(self.samples.len());
        let start = self.samples.len().saturating_sub(take);
        let mut items = Vec::new();
        for sample in self.samples.iter().skip(start) {
            items.push(format!(
                "{{\"ts\":{},\"visible\":{},\"streaming\":{},\"batch\":{},\"geom\":\"{}\",\"prepare\":{},\"draw\":{},\"total\":{}}}",
                sample.timestamp_ms,
                sample.visible_hash,
                sample.streaming_hash,
                sample.batch_hash,
                sample.geometry.replace('"', "'"),
                sample.prepare_ms,
                sample.draw_ms,
                sample.total_ms,
            ));
        }
        let mut events = Vec::new();
        for event in &self.events {
            events.push(format!(
                "{{\"ts\":{},\"kind\":\"{}\",\"prev\":{},\"current\":{}}}",
                event.timestamp_ms,
                event.kind.replace('"', "'"),
                event.prev,
                event.current,
            ));
        }
        format!(
            "{{\"samples\":[{}],\"events\":[{}],\"sample_count\":{},\"event_count\":{}}}",
            items.join(","),
            events.join(","),
            self.samples.len(),
            self.events.len(),
        )
    }

    /// Recent interface notice when flicker recurs, if any.
    pub fn recent_notice(&self) -> Option<FlickerUiNotice> {
        if self.samples.is_empty() {
            return None;
        }
        Some(FlickerUiNotice {
            summary: format!("flicker detected ({} frames)", self.samples.len()),
            hint: "logs: flicker-frames · debug: key-to-draw flicker history".to_string(),
        })
    }

    /// Copy target label for `key` when flicker history exists. The `z` key
    /// copies the recent flicker payload for bug reports.
    pub fn copy_target_for_key(&self, key: char) -> Option<String> {
        if key == 'z' && !self.samples.is_empty() {
            Some("flicker-frames".to_string())
        } else {
            None
        }
    }
}

/// Key-to-paint latency history with input attribution and quantiles.
#[derive(Debug, Clone, Default)]
pub struct KeyPaintHistory {
    pending_input_ms: Option<u64>,
    pending_event: FrameInputAttribution,
    samples: VecDeque<u64>,
}

impl KeyPaintHistory {
    /// Empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Note an input arrival with its attribution.
    pub fn note_key_event_read(&mut self, now_ms: u64, event: FrameInputAttribution) {
        self.pending_input_ms = Some(now_ms);
        self.pending_event = event;
    }

    /// Note a painted frame, returning the sample latency when input was pending.
    pub fn note_frame_painted(&mut self, now_ms: u64) -> Option<u64> {
        let start = self.pending_input_ms.take()?;
        let latency = now_ms.saturating_sub(start);
        self.samples.push_back(latency);
        while self.samples.len() > KEY_TO_PAINT_MAX_SAMPLES {
            self.samples.pop_front();
        }
        Some(latency)
    }

    /// Retained sample count.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no latency was recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Clear pending input and retained samples.
    pub fn clear(&mut self) {
        self.pending_input_ms = None;
        self.pending_event = FrameInputAttribution::empty();
        self.samples.clear();
    }

    fn sorted(&self) -> Vec<u64> {
        sorted_clone(&self.samples)
    }

    /// Median latency, if any input was painted.
    pub fn p50_ms(&self) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        Some(percentile_u64(&self.sorted(), 0.5))
    }

    /// 95th percentile latency, if any input was painted.
    pub fn p95_ms(&self) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        Some(percentile_u64(&self.sorted(), 0.95))
    }

    /// Maximum latency, if any input was painted.
    pub fn max_ms(&self) -> Option<u64> {
        self.samples.iter().copied().max()
    }

    /// Mean latency, if any input was painted.
    pub fn mean_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let total: u64 = self.samples.iter().sum();
        Some(total as f64 / self.samples.len() as f64)
    }

    /// Debug JSON with samples plus distribution summaries.
    pub fn debug_json(&self) -> String {
        let mut items = Vec::new();
        for sample in &self.samples {
            items.push(sample.to_string());
        }
        format!(
            "{{\"samples\":[{}],\"count\":{},\"p50\":{},\"p95\":{},\"max\":{},\"mean\":{:.2},\"pending_event\":\"{}\"}}",
            items.join(","),
            self.samples.len(),
            self.p50_ms().unwrap_or(0),
            self.p95_ms().unwrap_or(0),
            self.max_ms().unwrap_or(0),
            self.mean_ms().unwrap_or(0.0),
            self.pending_event.event.replace('"', "'"),
        )
    }
}

/// One anchor jarring event with appearance detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarringEvent {
    /// Caller-supplied clock for the event.
    pub timestamp_ms: u64,
    /// Shift class for the transition.
    pub kind: AnchorShift,
    /// New rows that appeared in this transition.
    pub appeared: usize,
    /// Largest contiguous appeared block in rows.
    pub largest_block: usize,
}

/// Anchor stability report with per-minute rates and appearance quantiles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnchorStabilityReport {
    /// Frames observed (including the first baseline).
    pub frames_observed: u64,
    /// Transitions compared (observed minus one, at minimum zero).
    pub frames_compared: u64,
    /// Stable transitions.
    pub stable: u64,
    /// Relocated transitions.
    pub relocated: u64,
    /// Inserted-above transitions.
    pub inserted_above: u64,
    /// Large-jump transitions.
    pub large_jump: u64,
    /// Full-rearrangement transitions.
    pub rearranged: u64,
    /// Mean appeared rows per compared frame.
    pub appeared_mean: f64,
    /// 95th percentile appeared rows.
    pub appeared_p95: usize,
    /// Maximum appeared rows.
    pub appeared_max: usize,
    /// Largest appeared block across compared frames.
    pub largest_block_max: usize,
    /// Wall span in milliseconds from first to last observation.
    pub span_ms: u64,
    /// Jarring events (non-stable transitions) per minute.
    pub jarring_per_minute: f64,
    /// Recent jarring events, oldest first, capped.
    pub recent_events: Vec<JarringEvent>,
}

impl AnchorStabilityReport {
    /// One-line summary for logs.
    pub fn summary(&self) -> String {
        format!(
            "observed={} compared={} stable={} jarring_per_min={:.2} appeared_mean={:.1} p95={} max={} block_max={}",
            self.frames_observed,
            self.frames_compared,
            self.stable,
            self.jarring_per_minute,
            self.appeared_mean,
            self.appeared_p95,
            self.appeared_max,
            self.largest_block_max,
        )
    }
}

/// Always-on anchor monitor: hashes rendered frames, classifies shifts,
/// tracks appeared-row distributions and caps jarring events.
#[derive(Debug, Clone, Default)]
pub struct AnchorMonitor {
    previous: Option<AnchorFrame>,
    first_ms: Option<u64>,
    last_ms: Option<u64>,
    observed: u64,
    stable: u64,
    relocated: u64,
    inserted_above: u64,
    large_jump: u64,
    rearranged: u64,
    appeared: Vec<u64>,
    largest_blocks: Vec<u64>,
    events: VecDeque<JarringEvent>,
}

impl AnchorMonitor {
    /// Empty monitor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe one frame at `now_ms`, returning the shift class. The first
    /// frame only seeds the baseline and reports stable.
    pub fn observe_frame(
        &mut self,
        frame: AnchorFrame,
        viewport_h: usize,
        now_ms: u64,
    ) -> AnchorShift {
        self.observed += 1;
        if self.first_ms.is_none() {
            self.first_ms = Some(now_ms);
        }
        self.last_ms = Some(now_ms);
        let Some(prev) = self.previous.clone() else {
            self.previous = Some(frame);
            return AnchorShift::Stable;
        };
        let shift = classify_shift(&prev, &frame, viewport_h);
        let (appeared, largest) = appeared_detail(&prev, &frame);
        self.appeared.push(appeared as u64);
        self.largest_blocks.push(largest as u64);
        match shift {
            AnchorShift::Stable => self.stable += 1,
            AnchorShift::Relocated => self.relocated += 1,
            AnchorShift::InsertedAbove => self.inserted_above += 1,
            AnchorShift::LargeJump => self.large_jump += 1,
            AnchorShift::FullRearrangement => self.rearranged += 1,
        }
        if shift != AnchorShift::Stable {
            self.events.push_back(JarringEvent {
                timestamp_ms: now_ms,
                kind: shift,
                appeared,
                largest_block: largest,
            });
            while self.events.len() > ANCHOR_EVENTS_MAX_SAMPLES {
                self.events.pop_front();
            }
        }
        self.previous = Some(frame);
        shift
    }

    /// Reset the monitor to the empty baseline.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Current stability report with rates and quantiles.
    pub fn report(&self) -> AnchorStabilityReport {
        let compared = self.observed.saturating_sub(1);
        let mut appeared_sorted = self.appeared.clone();
        appeared_sorted.sort_unstable();
        let appeared_max = appeared_sorted.last().copied().unwrap_or(0) as usize;
        let appeared_p95 = percentile_u64(&appeared_sorted, 0.95) as usize;
        let appeared_mean = if appeared_sorted.is_empty() {
            0.0
        } else {
            appeared_sorted.iter().sum::<u64>() as f64 / appeared_sorted.len() as f64
        };
        let largest_block_max = self.largest_blocks.iter().copied().max().unwrap_or(0) as usize;
        let span_ms = match (self.first_ms, self.last_ms) {
            (Some(first), Some(last)) => last.saturating_sub(first),
            _ => 0,
        };
        let jarring =
            (self.relocated + self.inserted_above + self.large_jump + self.rearranged) as f64;
        let jarring_per_minute = if span_ms == 0 {
            jarring
        } else {
            jarring / (span_ms as f64 / 60_000.0)
        };
        AnchorStabilityReport {
            frames_observed: self.observed,
            frames_compared: compared,
            stable: self.stable,
            relocated: self.relocated,
            inserted_above: self.inserted_above,
            large_jump: self.large_jump,
            rearranged: self.rearranged,
            appeared_mean,
            appeared_p95,
            appeared_max,
            largest_block_max,
            span_ms,
            jarring_per_minute,
            recent_events: self.events.iter().cloned().collect(),
        }
    }

    /// Report serialized to JSON for debug and demo entries.
    pub fn report_json(&self) -> String {
        let report = self.report();
        let mut events = Vec::new();
        for event in &report.recent_events {
            events.push(format!(
                "{{\"ts\":{},\"kind\":\"{:?}\",\"appeared\":{},\"block\":{}}}",
                event.timestamp_ms, event.kind, event.appeared, event.largest_block,
            ));
        }
        format!(
            "{{\"observed\":{},\"compared\":{},\"stable\":{},\"relocated\":{},\"inserted\":{},\"jump\":{},\"rearranged\":{},\"appeared_mean\":{:.2},\"appeared_p95\":{},\"appeared_max\":{},\"block_max\":{},\"span_ms\":{},\"jarring_per_min\":{:.2},\"events\":[{}]}}",
            report.frames_observed,
            report.frames_compared,
            report.stable,
            report.relocated,
            report.inserted_above,
            report.large_jump,
            report.rearranged,
            report.appeared_mean,
            report.appeared_p95,
            report.appeared_max,
            report.largest_block_max,
            report.span_ms,
            report.jarring_per_minute,
            events.join(","),
        )
    }
}

fn appeared_detail(prev: &AnchorFrame, next: &AnchorFrame) -> (usize, usize) {
    let known: HashSet<u64> = prev.row_hashes.iter().copied().collect();
    let mut appeared = 0usize;
    let mut largest = 0usize;
    let mut run = 0usize;
    for hash in &next.row_hashes {
        if *hash == crate::anchor::BLANK_ROW_HASH || known.contains(hash) {
            if run > largest {
                largest = run;
            }
            run = 0;
        } else {
            appeared += 1;
            run += 1;
            if run > largest {
                largest = run;
            }
        }
    }
    (appeared, largest)
}

/// Replay time coverage: deterministic wall span plus event-type and bus
/// coverage plus tick completeness.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayCoverage {
    /// Wall span in milliseconds from first to last timestamped event.
    pub span_ms: u64,
    /// Key events observed.
    pub key_events: usize,
    /// Resize events observed.
    pub resize_events: usize,
    /// Delta events observed.
    pub delta_events: usize,
    /// Bus events observed.
    pub bus_events: usize,
    /// Tick events observed.
    pub tick_events: usize,
    /// Distinct bus topics observed, sorted.
    pub bus_topics: Vec<String>,
    /// True when at least one tick was recorded.
    pub tick_complete: bool,
}

impl ReplayCoverage {
    /// One-line summary for logs.
    pub fn summary(&self) -> String {
        format!(
            "span_ms={} key={} resize={} delta={} bus={} tick={} topics={} complete={}",
            self.span_ms,
            self.key_events,
            self.resize_events,
            self.delta_events,
            self.bus_events,
            self.tick_events,
            self.bus_topics.len(),
            self.tick_complete,
        )
    }

    /// Coverage serialized to JSON for debug and demo entries.
    pub fn report_json(&self) -> String {
        let mut topics = Vec::new();
        for topic in &self.bus_topics {
            topics.push(format!("\"{}\"", topic.replace('"', "'")));
        }
        format!(
            "{{\"span_ms\":{},\"key\":{},\"resize\":{},\"delta\":{},\"bus\":{},\"tick\":{},\"topics\":[{}],\"complete\":{}}}",
            self.span_ms,
            self.key_events,
            self.resize_events,
            self.delta_events,
            self.bus_events,
            self.tick_events,
            topics.join(","),
            self.tick_complete,
        )
    }
}

/// Deterministic replay time coverage over `events`. Timestamps come from the
/// recorded `at_ms` values, so reruns of the same sequence report identical
/// spans without touching the wall clock.
pub fn replay_time_coverage(events: &[RecordedEvent]) -> ReplayCoverage {
    let mut coverage = ReplayCoverage::default();
    let mut topics: HashSet<String> = HashSet::new();
    let mut min_ms: Option<u64> = None;
    let mut max_ms: Option<u64> = None;
    for event in events {
        match event {
            RecordedEvent::Key { at_ms, .. } => {
                coverage.key_events += 1;
                min_ms = Some(min_ms.map_or(*at_ms, |low| low.min(*at_ms)));
                max_ms = Some(max_ms.map_or(*at_ms, |high| high.max(*at_ms)));
            }
            RecordedEvent::Resize { at_ms, .. } => {
                coverage.resize_events += 1;
                min_ms = Some(min_ms.map_or(*at_ms, |low| low.min(*at_ms)));
                max_ms = Some(max_ms.map_or(*at_ms, |high| high.max(*at_ms)));
            }
            RecordedEvent::Delta { at_ms, .. } => {
                coverage.delta_events += 1;
                min_ms = Some(min_ms.map_or(*at_ms, |low| low.min(*at_ms)));
                max_ms = Some(max_ms.map_or(*at_ms, |high| high.max(*at_ms)));
            }
            RecordedEvent::Bus { topic, at_ms } => {
                coverage.bus_events += 1;
                topics.insert(topic.clone());
                min_ms = Some(min_ms.map_or(*at_ms, |low| low.min(*at_ms)));
                max_ms = Some(max_ms.map_or(*at_ms, |high| high.max(*at_ms)));
            }
            RecordedEvent::Tick(value) => {
                coverage.tick_events += 1;
                min_ms = Some(min_ms.map_or(*value, |low| low.min(*value)));
                max_ms = Some(max_ms.map_or(*value, |high| high.max(*value)));
            }
        }
    }
    coverage.span_ms = match (min_ms, max_ms) {
        (Some(low), Some(high)) => high.saturating_sub(low),
        _ => 0,
    };
    let mut sorted: Vec<String> = topics.into_iter().collect();
    sorted.sort();
    coverage.bus_topics = sorted;
    coverage.tick_complete = coverage.tick_events > 0;
    coverage
}

/// Glyph atlas key-space counter: distinct `symbol x style` pairs that would
/// occupy atlas entries. Quantization keeps this bounded; the regression
/// test asserts the upper bound instead of disabling animation.
#[derive(Debug, Clone, Default)]
pub struct GlyphAtlasCounter {
    keys: HashSet<(String, u64)>,
    total: u64,
}

impl GlyphAtlasCounter {
    /// Empty counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one cell with its style identity hash.
    pub fn record(&mut self, symbol: &str, style_hash: u64) {
        self.total += 1;
        self.keys.insert((symbol.to_string(), style_hash));
        if self.keys.len() > 4096 {
            // Keep the counter bounded: drop an arbitrary entry rather than
            // growing without limit on pathological streams.
            if let Some(first) = self.keys.iter().next().cloned() {
                self.keys.remove(&first);
            }
        }
    }

    /// Distinct atlas keys observed.
    pub fn distinct(&self) -> usize {
        self.keys.len()
    }

    /// Total cells observed.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// True when the distinct key space stays within `limit`.
    pub fn within_bound(&self, limit: usize) -> bool {
        self.distinct() <= limit
    }
}

/// Distinct `symbol x style` atlas keys for `cells` without retaining state.
pub fn atlas_key_space_upper_bound(cells: &[(String, u64)]) -> usize {
    let mut keys: HashSet<(&str, u64)> = HashSet::new();
    for (symbol, style) in cells {
        keys.insert((symbol.as_str(), *style));
    }
    keys.len()
}

/// Observability budgets for regression verdicts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObservabilityBudgets {
    /// Maximum draw p95 wall time in milliseconds.
    pub max_draw_p95_ms: u64,
    /// Maximum slow frames retained.
    pub max_slow_frames: usize,
    /// Maximum key-to-paint p95 latency in milliseconds.
    pub max_key_p95_ms: u64,
    /// Maximum jarring anchor events per minute.
    pub max_jarring_per_min: f64,
}

impl Default for ObservabilityBudgets {
    fn default() -> Self {
        Self {
            max_draw_p95_ms: 50,
            max_slow_frames: 4,
            max_key_p95_ms: 150,
            max_jarring_per_min: 12.0,
        }
    }
}

/// Observability regression verdict across every history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityVerdict {
    /// True when every budget held.
    pub passed: bool,
    /// One line per violated budget.
    pub violations: Vec<String>,
    /// Full summary for logs.
    pub summary: String,
}

/// Check every observability history against `budgets`.
pub fn check_observability_regression(
    draws: &DrawCallHistory,
    slow: &SlowFrameHistory,
    keys: &KeyPaintHistory,
    anchor: &AnchorMonitor,
    budgets: &ObservabilityBudgets,
) -> ObservabilityVerdict {
    let mut violations = Vec::new();
    if draws.p95_total_ms() > budgets.max_draw_p95_ms {
        violations.push(format!(
            "draw p95={} over {}",
            draws.p95_total_ms(),
            budgets.max_draw_p95_ms
        ));
    }
    if slow.len() > budgets.max_slow_frames {
        violations.push(format!(
            "slow frames={} over {}",
            slow.len(),
            budgets.max_slow_frames
        ));
    }
    if keys.p95_ms().unwrap_or(0) > budgets.max_key_p95_ms {
        violations.push(format!(
            "key p95={} over {}",
            keys.p95_ms().unwrap_or(0),
            budgets.max_key_p95_ms
        ));
    }
    if anchor.report().jarring_per_minute > budgets.max_jarring_per_min {
        violations.push(format!(
            "jarring per min={:.2} over {:.2}",
            anchor.report().jarring_per_minute,
            budgets.max_jarring_per_min
        ));
    }
    let passed = violations.is_empty();
    let summary = format!(
        "draw_p95={} slow={} key_p95={} jarring={:.2} | {}",
        draws.p95_total_ms(),
        slow.len(),
        keys.p95_ms().unwrap_or(0),
        anchor.report().jarring_per_minute,
        if passed { "PASS" } else { "FAIL" },
    );
    ObservabilityVerdict {
        passed,
        violations,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw_at(ts: u64, total: u64) -> DrawCallAttribution {
        DrawCallAttribution {
            timestamp_ms: ts,
            total_ms: total,
            render_ms: total / 2,
            backend_flush_ms: 1,
            changed_cells: 10,
            total_cells: 100,
            force_full_redraw: false,
            input: FrameInputAttribution::empty(),
        }
    }

    #[test]
    fn draw_history_reports_distributions_and_json() {
        let mut history = DrawCallHistory::new();
        assert!(history.is_empty());
        for index in 0..10u64 {
            history.record(draw_at(index * 10, index + 1));
        }
        assert_eq!(history.len(), 10);
        assert!(history
            .mean_total_ms()
            .is_some_and(|mean| (mean - 5.5).abs() < 1e-9));
        assert_eq!(history.p50_total_ms(), 5);
        assert_eq!(history.p95_total_ms(), 10);
        assert_eq!(history.max_total_ms(), 10);
        assert!(history.mean_changed_ratio() > 0.0);
        assert!(history.draws_per_second(1_000) > 0.0);
        let json = history.debug_json(4);
        assert!(json.contains("p95_total"));
        assert!(json.contains("changed_ratio"));
    }

    #[test]
    fn draw_history_caps_at_240_samples() {
        let mut history = DrawCallHistory::new();
        for index in 0..300u64 {
            history.record(draw_at(index, 2));
        }
        assert_eq!(history.len(), DRAW_CALL_HISTORY_MAX_SAMPLES);
    }

    #[test]
    fn slow_history_tracks_splits_and_resources() {
        let mut history = SlowFrameHistory::new();
        assert!(history.is_empty());
        history.record(SlowFrameSample {
            timestamp_ms: 100,
            threshold_ms: slow_frame_threshold_ms(),
            prepare_ms: 20,
            draw_ms: 25,
            total_ms: 45,
            resource: FrameResourceAttribution::wall_only(45),
            input: FrameInputAttribution::for_key("enter"),
            scroll_delta: 0,
        });
        assert_eq!(history.len(), 1);
        assert_eq!(history.max_total_ms(), 45);
        assert_eq!(history.max_prepare_ms(), 20);
        assert_eq!(history.max_draw_ms(), 25);
        assert!(history.debug_json().contains("prepare"));
        let sampled = FrameResourceAttribution::sample(12);
        assert_eq!(sampled.wall_ms, 12);
    }

    #[test]
    fn flicker_history_reports_notice_and_copy_key() {
        let mut history = FlickerFrameHistory::new();
        assert!(history.recent_notice().is_none());
        assert_eq!(history.copy_target_for_key('z'), None);
        history.record_sample(FlickerFrameSample {
            timestamp_ms: 7,
            visible_hash: 11,
            streaming_hash: 0,
            batch_hash: 0,
            geometry: "80x24".to_string(),
            prepare_ms: 1,
            draw_ms: 2,
            total_ms: 3,
        });
        history.record_event(FlickerEvent {
            timestamp_ms: 7,
            kind: "flash".to_string(),
            prev: 11,
            current: 12,
        });
        assert_eq!(history.sample_len(), 1);
        assert_eq!(history.event_len(), 1);
        let notice = history.recent_notice().expect("notice when flicker recurs");
        assert!(notice.summary.contains("flicker"));
        assert!(!notice.hint.is_empty());
        assert_eq!(
            history.copy_target_for_key('z').as_deref(),
            Some("flicker-frames")
        );
        assert_eq!(history.copy_target_for_key('x'), None);
        assert!(history.debug_json(8).contains("visible"));
    }

    #[test]
    fn key_history_reports_p50_p95_max_and_json() {
        let mut history = KeyPaintHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.p50_ms(), None);
        history.note_key_event_read(100, FrameInputAttribution::for_key("enter"));
        assert_eq!(history.note_frame_painted(140), Some(40));
        history.note_key_event_read(200, FrameInputAttribution::for_key("a"));
        assert_eq!(history.note_frame_painted(260), Some(60));
        assert_eq!(history.len(), 2);
        assert_eq!(history.p50_ms(), Some(40));
        assert_eq!(history.p95_ms(), Some(60));
        assert_eq!(history.max_ms(), Some(60));
        assert!(history.debug_json().contains("p95"));
    }

    #[test]
    fn anchor_monitor_reports_rate_and_quantiles() {
        let mut monitor = AnchorMonitor::new();
        let rows = |words: &[&str]| {
            AnchorFrame::from_rows(&words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>())
        };
        assert_eq!(
            monitor.observe_frame(rows(&["a", "b", "c", "d"]), 4, 0),
            AnchorShift::Stable
        );
        monitor.observe_frame(rows(&["a", "b", "c", "d"]), 4, 1_000);
        monitor.observe_frame(rows(&["x", "y", "z", "w"]), 4, 61_000);
        let report = monitor.report();
        assert_eq!(report.frames_observed, 3);
        assert_eq!(report.frames_compared, 2);
        assert!(report.appeared_max > 0);
        assert!(report.jarring_per_minute > 0.0);
        assert!(!report.recent_events.is_empty());
        assert!(report.summary().contains("observed=3"));
        assert!(monitor.report_json().contains("jarring_per_min"));
        monitor.reset();
        assert_eq!(monitor.report().frames_observed, 0);
    }

    #[test]
    fn replay_coverage_spans_time_deterministically() {
        let events = vec![
            RecordedEvent::Key {
                label: "enter".to_string(),
                at_ms: 10,
            },
            RecordedEvent::Delta {
                text: "hi".to_string(),
                at_ms: 30,
            },
            RecordedEvent::Bus {
                topic: "fetch".to_string(),
                at_ms: 50,
            },
            RecordedEvent::Tick(70),
        ];
        let coverage = replay_time_coverage(&events);
        assert_eq!(coverage.span_ms, 60);
        assert_eq!(coverage.key_events, 1);
        assert_eq!(coverage.bus_topics, vec!["fetch".to_string()]);
        assert!(coverage.tick_complete);
        assert!(coverage.summary().contains("span_ms=60"));
        assert!(coverage.report_json().contains("topics"));
        let twice = replay_time_coverage(&events);
        assert_eq!(coverage, twice);
    }

    #[test]
    fn atlas_counter_bounds_distinct_keys() {
        let mut counter = GlyphAtlasCounter::new();
        counter.record("a", 1);
        counter.record("a", 1);
        counter.record("b", 2);
        assert_eq!(counter.distinct(), 2);
        assert_eq!(counter.total(), 3);
        assert!(counter.within_bound(8));
        assert!(!counter.within_bound(1));
        let cells = vec![
            ("a".to_string(), 1u64),
            ("a".to_string(), 1u64),
            ("b".to_string(), 2u64),
        ];
        assert_eq!(atlas_key_space_upper_bound(&cells), 2);
    }

    #[test]
    fn observability_regression_fails_on_degraded_draws() {
        let mut draws = DrawCallHistory::new();
        draws.record(draw_at(0, 200));
        let slow = SlowFrameHistory::new();
        let keys = KeyPaintHistory::new();
        let anchor = AnchorMonitor::new();
        let verdict = check_observability_regression(
            &draws,
            &slow,
            &keys,
            &anchor,
            &ObservabilityBudgets::default(),
        );
        assert!(!verdict.passed);
        assert!(!verdict.violations.is_empty());
        assert!(verdict.summary.contains("FAIL"));
    }

    #[test]
    fn percentile_handles_edges() {
        assert_eq!(percentile_u64(&[], 0.95), 0);
        assert_eq!(percentile_u64(&[7], 0.95), 7);
        let sorted = vec![1u64, 2, 3, 4];
        assert_eq!(percentile_u64(&sorted, 0.5), 2);
        assert_eq!(percentile_u64(&sorted, 1.0), 4);
    }

    #[test]
    fn input_attribution_labels_keys() {
        let attribution = FrameInputAttribution::for_key("enter");
        assert_eq!(attribution.event, "key:enter");
    }
}
