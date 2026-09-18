//! Streaming rate pacer between delta arrival and visible text.
//!
//! Bursty provider output must not jump straight onto the screen: small
//! high-frequency deltas cause repeated layouts while large bursts jump
//! several rows in one frame. [`StreamPacer`] keeps the not-yet-visible
//! backlog and releases it over time at a backlog-proportional rate, so each
//! frame reveals a bounded run of characters and an idle backlog naturally
//! catches up. Completion and interrupt paths drain the backlog so the
//! settled content never waits on the rate limiter.
//!
//! [`SegmentedPacer`] layers an arrival-ordered queue over two pacers so
//! answer text and reasoning deltas share one release order with a
//! zero-width reasoning-close barrier.

use std::collections::VecDeque;

/// Baseline reveal rate (chars per second).
pub const BASE_REVEAL_CPS: f64 = 180.0;
/// Extra rate per backlog character.
pub const REVEAL_BACKLOG_GAIN: f64 = 3.0;
/// Hard cap on the reveal rate.
pub const MAX_REVEAL_CPS: f64 = 960.0;
/// Single advance never accounts more than this much time, so a stall does
/// not dump its whole backlog in the next frame.
pub const MAX_REVEAL_STEP_MS: u64 = 50;

/// Tunable pacer rates; defaults mirror the reference magnitudes.
#[derive(Debug, Clone, Copy)]
pub struct PacerConfig {
    /// Baseline reveal rate.
    pub base_cps: f64,
    /// Gain applied per backlog character.
    pub backlog_gain: f64,
    /// Hard rate cap.
    pub max_cps: f64,
    /// Per-advance time cap in milliseconds.
    pub max_step_ms: u64,
}

impl Default for PacerConfig {
    fn default() -> Self {
        Self {
            base_cps: BASE_REVEAL_CPS,
            backlog_gain: REVEAL_BACKLOG_GAIN,
            max_cps: MAX_REVEAL_CPS,
            max_step_ms: MAX_REVEAL_STEP_MS,
        }
    }
}

/// Canonical reasoning-segment contract lives in the markdown crate so the
/// pacer and the renderer share one sentinel and escape set.
pub use tui_markdown::markdown::reasoning::{
    escape_reasoning, mark_reasoning, reasoning_line_content, split_reasoning_marks,
    REASONING_ESCAPES, REASONING_SENTINEL,
};

/// Arrival vs display decoupling: `buffer` holds every arrived byte while
/// `released` marks the visible prefix length. `fed` is not stored here;
/// callers track how much of the visible prefix they already consumed.
#[derive(Debug, Clone)]
pub struct StreamPacer {
    config: PacerConfig,
    buffer: String,
    released: usize,
    last_advance_ms: Option<u64>,
    bypass: bool,
}

impl Default for StreamPacer {
    fn default() -> Self {
        Self::new(PacerConfig::default())
    }
}

impl StreamPacer {
    /// New pacer with explicit rates.
    pub fn new(config: PacerConfig) -> Self {
        Self {
            config,
            buffer: String::new(),
            released: 0,
            last_advance_ms: None,
            bypass: false,
        }
    }

    /// Direct pass-through mode keeps the interface while disabling pacing;
    /// used as the rollback when any pacing delay is unacceptable.
    pub fn set_bypass(&mut self, bypass: bool) {
        self.bypass = bypass;
        if bypass {
            self.released = self.buffer.len();
        }
    }

    /// True when pacing is disabled and arrival equals visibility.
    pub fn is_bypass(&self) -> bool {
        self.bypass
    }

    /// Queue an arrival without making it visible yet.
    pub fn push(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.buffer.push_str(delta);
        if self.bypass {
            self.released = self.buffer.len();
        }
    }

    /// Advance the visible prefix toward `now_ms` and return newly visible
    /// bytes. The clock is caller-supplied so tests assert exact per-frame
    /// curves without sleeping.
    pub fn advance(&mut self, now_ms: u64) -> usize {
        let before = self.released;
        if self.bypass {
            self.released = self.buffer.len();
            self.last_advance_ms = Some(now_ms);
            return self.released.saturating_sub(before);
        }
        let Some(last) = self.last_advance_ms else {
            self.last_advance_ms = Some(now_ms);
            return 0;
        };
        let mut dt_ms = now_ms.saturating_sub(last);
        if dt_ms == 0 || self.released >= self.buffer.len() {
            self.last_advance_ms = Some(now_ms);
            return 0;
        }
        dt_ms = dt_ms.min(self.config.max_step_ms);
        let backlog = self.buffer.len().saturating_sub(self.released) as f64;
        let cps = (self.config.base_cps + backlog * self.config.backlog_gain)
            .min(self.config.max_cps)
            .max(1.0);
        let mut allow = (f64::from(dt_ms as u32) / 1000.0 * cps).floor() as usize;
        allow = allow.max(1).min(self.buffer.len() - self.released);
        let mut end = self.released + allow;
        while end > self.released && !self.buffer.is_char_boundary(end) {
            end -= 1;
        }
        if end == self.released {
            end = next_char_boundary(&self.buffer, self.released).min(self.buffer.len());
        }
        self.released = end;
        self.last_advance_ms = Some(now_ms);
        self.released.saturating_sub(before)
    }

    /// Currently visible prefix.
    pub fn visible(&self) -> &str {
        &self.buffer[..self.released.min(self.buffer.len())]
    }

    /// Visible prefix length in bytes.
    pub fn visible_len(&self) -> usize {
        self.released.min(self.buffer.len())
    }

    /// Queued but not yet visible bytes.
    pub fn pending_len(&self) -> usize {
        self.buffer.len().saturating_sub(self.released)
    }

    /// Total arrived bytes.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Release everything immediately; completion and interrupt paths call
    /// this so settled semantics never wait on the limiter.
    pub fn drain(&mut self) -> usize {
        let before = self.released;
        self.released = self.buffer.len();
        self.released.saturating_sub(before)
    }

    /// Drop the released prefix once the consumer has fully consumed it.
    /// Keeps the buffer bounded on long streams.
    pub fn compact(&mut self, consumed_through: usize) {
        let drop = consumed_through.min(self.released).min(self.buffer.len());
        if drop == 0 {
            return;
        }
        self.buffer.drain(..drop);
        self.released -= drop;
    }

    /// Reset for a new turn.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.released = 0;
        self.last_advance_ms = None;
    }
}

fn next_char_boundary(text: &str, from: usize) -> usize {
    let mut end = from + 1;
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    end.min(text.len())
}

/// One arrival-ordered queue entry for the segmented pacer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueEntry {
    /// Answer-text chunk of `len` bytes appended to the text pacer.
    Text(usize),
    /// Reasoning chunk of `len` bytes appended to the reasoning pacer.
    Reasoning(usize),
    /// Zero-width reasoning-close barrier preserving arrival order.
    CloseReasoning,
}

/// Visible operation released by the segmented pacer in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacerOp {
    /// Newly visible answer text since the previous take.
    Text(String),
    /// Newly visible reasoning text since the previous take.
    Reasoning(String),
    /// Reasoning stream closed; carries no bytes.
    CloseReasoning,
}

/// Arrival-ordered pacer over answer text and reasoning deltas.
///
/// Both segments advance under the same caller-supplied clock at the
/// configured rate, but release follows the arrival queue: a text burst at
/// the head blocks later reasoning until its bytes turn visible, and vice
/// versa. Completion paths drain both pacers so settlement never waits on
/// the limiter. Direct single-stream use stays on [`StreamPacer`].
#[derive(Debug)]
pub struct SegmentedPacer {
    text: StreamPacer,
    reasoning: StreamPacer,
    order: VecDeque<QueueEntry>,
    text_taken: usize,
    reasoning_taken: usize,
}

impl Default for SegmentedPacer {
    fn default() -> Self {
        Self::new(PacerConfig::default())
    }
}

impl SegmentedPacer {
    /// New segmented pacer sharing one rate configuration across segments.
    pub fn new(config: PacerConfig) -> Self {
        Self {
            text: StreamPacer::new(config),
            reasoning: StreamPacer::new(config),
            order: VecDeque::new(),
            text_taken: 0,
            reasoning_taken: 0,
        }
    }

    /// Direct pass-through mode keeps the interface while disabling pacing.
    pub fn set_bypass(&mut self, bypass: bool) {
        self.text.set_bypass(bypass);
        self.reasoning.set_bypass(bypass);
    }

    /// True when pacing is disabled and arrival equals visibility.
    pub fn is_bypass(&self) -> bool {
        self.text.is_bypass()
    }

    /// Queue answer-text arrival without making it visible yet.
    pub fn push_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.text.push(delta);
        self.order.push_back(QueueEntry::Text(delta.len()));
    }

    /// Queue reasoning arrival without making it visible yet.
    pub fn push_reasoning(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.reasoning.push(delta);
        self.order.push_back(QueueEntry::Reasoning(delta.len()));
    }

    /// Queue a zero-width reasoning-close barrier in arrival position.
    pub fn push_close_reasoning(&mut self) {
        self.order.push_back(QueueEntry::CloseReasoning);
    }

    /// Advance both segments toward `now_ms`.
    pub fn advance(&mut self, now_ms: u64) {
        self.text.advance(now_ms);
        self.reasoning.advance(now_ms);
    }

    /// Release everything immediately on both segments.
    pub fn drain(&mut self) {
        self.text.drain();
        self.reasoning.drain();
    }

    /// Queued but not yet visible bytes across both segments.
    pub fn pending_len(&self) -> usize {
        self.text.pending_len() + self.reasoning.pending_len()
    }

    /// Visible answer-text prefix length in bytes.
    pub fn text_visible_len(&self) -> usize {
        self.text.visible_len()
    }

    /// Reset for a new turn.
    pub fn clear(&mut self) {
        self.text.clear();
        self.reasoning.clear();
        self.order.clear();
        self.text_taken = 0;
        self.reasoning_taken = 0;
    }

    /// Take newly visible operations in arrival order. Each entry releases
    /// only the bytes its segment's cursor has actually uncovered since the
    /// previous take, so interleaved text and reasoning stay paced per frame
    /// and keep the arrival sequence; a partially revealed entry is kept in
    /// the queue with its remaining length. Consumed prefixes are compacted
    /// so long streams stay bounded.
    pub fn take_visible_ops(&mut self) -> Vec<PacerOp> {
        let mut ops = Vec::new();
        while let Some(entry) = self.order.front_mut() {
            match entry {
                QueueEntry::Text(len) => {
                    let visible = self.text.visible().to_string();
                    let avail = visible.len().saturating_sub(self.text_taken);
                    if avail == 0 {
                        break;
                    }
                    let take = (*len).min(avail);
                    let start = self.text_taken;
                    ops.push(PacerOp::Text(visible[start..start + take].to_string()));
                    self.text_taken += take;
                    *len -= take;
                    if *len == 0 {
                        self.order.pop_front();
                    }
                }
                QueueEntry::Reasoning(len) => {
                    let visible = self.reasoning.visible().to_string();
                    let avail = visible.len().saturating_sub(self.reasoning_taken);
                    if avail == 0 {
                        break;
                    }
                    let take = (*len).min(avail);
                    let start = self.reasoning_taken;
                    ops.push(PacerOp::Reasoning(visible[start..start + take].to_string()));
                    self.reasoning_taken += take;
                    *len -= take;
                    if *len == 0 {
                        self.order.pop_front();
                    }
                }
                QueueEntry::CloseReasoning => {
                    ops.push(PacerOp::CloseReasoning);
                    self.order.pop_front();
                }
            }
        }
        self.text.compact(self.text_taken);
        self.reasoning.compact(self.reasoning_taken);
        self.text_taken = 0;
        self.reasoning_taken = 0;
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_marks_round_trip_in_arrival_order() {
        let marked = format!(
            "answer {}thinking{} tail",
            REASONING_SENTINEL, REASONING_SENTINEL
        );
        assert_eq!(
            split_reasoning_marks(&marked),
            vec![
                (false, "answer ".to_string()),
                (true, "thinking".to_string()),
                (false, " tail".to_string()),
            ]
        );
        assert_eq!(
            mark_reasoning("thinking"),
            format!("{s}thinking{s}", s = REASONING_SENTINEL)
        );
    }

    #[test]
    fn reasoning_escapes_keep_emphasis_adjacent() {
        assert_eq!(escape_reasoning("a*b_c~d"), "a\\*b\\_c\\~d");
        assert_eq!(escape_reasoning("plain"), "plain");
        assert!(REASONING_ESCAPES.contains(&'*'));
    }

    #[test]
    fn paced_frames_follow_the_rate_curve() {
        let mut pacer = StreamPacer::default();
        pacer.push(&"x".repeat(600));
        assert_eq!(pacer.advance(0), 0);
        // 50ms capped step at ~960cps ceiling releases at most ~48 chars.
        let first = pacer.advance(1_000);
        assert!(first > 0 && first <= 60, "bounded reveal, got {first}");
        assert!(pacer.pending_len() > 0, "backlog remains after one step");
        // Repeated steps converge without ever dumping the backlog at once.
        for step in 1..40u64 {
            let newly = pacer.advance(1_000 + step * 50);
            assert!(newly <= 60, "per-frame bound holds, got {newly}");
            if pacer.pending_len() == 0 {
                break;
            }
        }
        assert_eq!(pacer.pending_len(), 0);
        assert_eq!(pacer.visible_len(), 600);
    }

    #[test]
    fn stall_does_not_dump_the_backlog() {
        let mut pacer = StreamPacer::default();
        pacer.push(&"y".repeat(400));
        pacer.advance(0);
        // A ten-second stall still releases only one capped step.
        let newly = pacer.advance(10_000);
        assert!(
            newly <= 60,
            "stall must not dump backlog at once, got {newly}"
        );
        assert!(pacer.pending_len() > 0);
    }

    #[test]
    fn drain_releases_everything_for_completion() {
        let mut pacer = StreamPacer::default();
        pacer.push("hello ");
        pacer.push("world");
        pacer.advance(0);
        assert!(pacer.pending_len() > 0);
        pacer.drain();
        assert_eq!(pacer.visible(), "hello world");
        assert_eq!(pacer.pending_len(), 0);
    }

    #[test]
    fn bypass_mode_is_a_straight_pass_through() {
        let mut pacer = StreamPacer::default();
        pacer.set_bypass(true);
        pacer.push("abc");
        assert_eq!(pacer.visible(), "abc");
        pacer.push("def");
        assert_eq!(pacer.visible(), "abcdef");
    }

    #[test]
    fn interleaved_arrivals_keep_order() {
        let mut pacer = StreamPacer::default();
        for chunk in ["answer one ", "reasoning bit ", "answer two"] {
            pacer.push(chunk);
        }
        pacer.drain();
        assert_eq!(pacer.visible(), "answer one reasoning bit answer two");
    }

    #[test]
    fn char_boundaries_are_never_split() {
        let mut pacer = StreamPacer::new(PacerConfig {
            base_cps: 10.0,
            backlog_gain: 0.0,
            max_cps: 10.0,
            max_step_ms: 50,
        });
        pacer.push("横横横横横横");
        pacer.advance(0);
        for step in 1..20u64 {
            pacer.advance(step * 50);
            assert!(
                pacer.visible().is_char_boundary(pacer.visible_len()),
                "visible must stay on char boundaries"
            );
        }
    }

    #[test]
    fn paced_stream_reassembles_identical_text() {
        use tui_markdown::markdown::MarkdownStream;
        let chunks = [
            "first para\n\n",
            "second ",
            "para\n- a\n",
            "- b\n\n",
            "```sh\necho hi\n```\n",
        ];
        // Direct path: every arrival parses immediately.
        let mut direct = MarkdownStream::default();
        let mut direct_text = String::new();
        for chunk in chunks {
            let frame = direct.push(chunk);
            direct_text.push_str(&frame.new_committed);
            direct_text.push_str(&frame.new_streaming);
        }
        direct_text.push_str(&direct.finish().new_committed);

        // Paced path: arrivals queue in the pacer, frames advance the
        // visible prefix into a throttled stream, completion drains.
        let mut pacer = StreamPacer::default();
        let mut paced = MarkdownStream::new_with_interval(64 * 1024, 0);
        let mut paced_text = String::new();
        let mut fed = 0usize;
        let mut now = 0u64;
        for chunk in chunks {
            pacer.push(chunk);
            now += 7;
            pacer.advance(now);
            let visible = pacer.visible().to_string();
            if visible.len() > fed {
                paced.push_throttled(&visible[fed..]);
                fed = visible.len();
            }
            if let Some(frame) = paced.prepare_frame(now, false) {
                paced_text.push_str(&frame.new_committed);
                paced_text.push_str(&frame.new_streaming);
            }
        }
        pacer.drain();
        let visible = pacer.visible().to_string();
        if visible.len() > fed {
            paced.push_throttled(&visible[fed..]);
        }
        if let Some(frame) = paced.prepare_frame(now + 1, true) {
            paced_text.push_str(&frame.new_committed);
            paced_text.push_str(&frame.new_streaming);
        }
        paced_text.push_str(&paced.finish().new_committed);
        assert_eq!(paced_text, direct_text);
        assert_eq!(pacer.visible(), chunks.concat());
    }

    #[test]
    fn segmented_pacer_keeps_arrival_order_across_kinds() {
        use super::PacerOp;
        let mut pacer = super::SegmentedPacer::default();
        pacer.push_text("answer one ");
        pacer.push_reasoning("thinking...");
        pacer.push_text("answer two");
        pacer.push_close_reasoning();
        pacer.drain();
        let ops = pacer.take_visible_ops();
        assert_eq!(
            ops,
            vec![
                PacerOp::Text("answer one ".to_string()),
                PacerOp::Reasoning("thinking...".to_string()),
                PacerOp::Text("answer two".to_string()),
                PacerOp::CloseReasoning,
            ]
        );
        assert_eq!(pacer.pending_len(), 0);
    }

    #[test]
    fn segmented_pacer_bounds_each_frame_and_converges() {
        let mut pacer = super::SegmentedPacer::default();
        pacer.push_text(&"x".repeat(400));
        pacer.push_reasoning(&"y".repeat(200));
        pacer.advance(0);
        for step in 1..60u64 {
            pacer.advance(step * 50);
            let ops = pacer.take_visible_ops();
            let newly: usize = ops
                .iter()
                .map(|op| match op {
                    super::PacerOp::Text(s) | super::PacerOp::Reasoning(s) => s.len(),
                    super::PacerOp::CloseReasoning => 0,
                })
                .sum();
            // Each leading segment is paced per frame. The trailing segment
            // only flushes once the leading one clears (arrival-order
            // blocking), so the settlement frame may exceed the per-frame
            // bound — exactly like an explicit `drain()`.
            assert!(
                newly <= 130 || pacer.pending_len() == 0,
                "per-frame bound holds, got {newly}"
            );
            if pacer.pending_len() == 0 {
                break;
            }
        }
        assert_eq!(pacer.pending_len(), 0);
    }

    #[test]
    fn segmented_bypass_passes_everything_through() {
        let mut pacer = super::SegmentedPacer::default();
        pacer.set_bypass(true);
        pacer.push_text("abc");
        pacer.push_reasoning("def");
        let ops = pacer.take_visible_ops();
        assert_eq!(ops.len(), 2);
        assert_eq!(pacer.pending_len(), 0);
    }
}
