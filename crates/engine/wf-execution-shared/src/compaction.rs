use serde::{Deserialize, Serialize};
use wf_types::message::Message;

/// Default budget ratios deriving absolute thresholds from the model window.
/// Deep-compact target, handoff line, compact line, hard-limit valve.
pub const H_TARGET_RATIO_DEFAULT: f64 = 0.30;
pub const T_HANDOFF_RATIO_DEFAULT: f64 = 0.55;
pub const T_COMPACT_RATIO_DEFAULT: f64 = 0.70;
pub const T_HARD_RATIO_DEFAULT: f64 = 0.85;

/// Smallest first-time compressible interval in tokens.
pub const R_MIN_RANGE_DEFAULT: u64 = 2000;
/// Smallest re-compressible summary size in tokens.
pub const R_REPRESS_DEFAULT: u64 = 5000;
/// Maximum re-compressions of a single block.
pub const K_REPRESS_DEFAULT: u32 = 2;
/// Recent rounds always kept visible in the hot view.
pub const RECENT_RAW_TURNS_DEFAULT: usize = 2;

/// One compression work item: a slice of the emitted snapshot identified by
/// snapshot indices, not by session sequence numbers. The emitter owns the
/// snapshot, so indices stay valid for the one signal they travel with. An
/// empty plan means the legacy single-summary mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionPlanItem {
    pub block_id: String,
    pub start: usize,
    pub end: usize,
    /// Zero for a first compression, greater for a re-compression used to
    /// pick the prompt variant on the service side.
    #[serde(default)]
    pub depth: u32,
}

impl CompactionPlanItem {
    /// Clamp the item interval to a snapshot length; `None` when the clamped
    /// interval is empty and the item carries no work.
    pub fn clamped_range(&self, len: usize) -> Option<(usize, usize)> {
        let start = self.start.min(len);
        let end = self.end.min(len);
        if start < end {
            Some((start, end))
        } else {
            None
        }
    }

    /// Slice the snapshot for this item; empty when the interval covers
    /// nothing after clamping.
    pub fn slice<'a>(&self, messages: &'a [Message]) -> &'a [Message] {
        match self.clamped_range(messages.len()) {
            Some((start, end)) => &messages[start..end],
            None => &[],
        }
    }
}

/// Absolute token thresholds derived from one model window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionThresholds {
    pub h_target: u64,
    pub t_handoff: u64,
    pub t_compact: u64,
    pub t_hard: u64,
}

/// Resolve absolute thresholds from a model window and the configured
/// ratios. Missing ratios fall back to the documented defaults so callers
/// holding a sparse config still agree on numbers; ratio ordering is checked
/// by config validation, and a zero window yields zero thresholds.
pub fn resolve_compaction_thresholds(
    window_tokens: u64,
    limits: &wf_types::config::limits::CompactionLimits,
) -> CompactionThresholds {
    let ratio = |value: Option<f64>, fallback: f64| value.unwrap_or(fallback);
    let scaled = |r: f64| (window_tokens as f64 * r) as u64;
    CompactionThresholds {
        h_target: scaled(ratio(limits.h_target_ratio, H_TARGET_RATIO_DEFAULT)),
        t_handoff: scaled(ratio(limits.t_handoff_ratio, T_HANDOFF_RATIO_DEFAULT)),
        t_compact: scaled(ratio(limits.t_compact_ratio, T_COMPACT_RATIO_DEFAULT)),
        t_hard: scaled(ratio(limits.t_hard_ratio, T_HARD_RATIO_DEFAULT)),
    }
}

/// Check the configured ratios follow the required strict ordering with
/// each ratio inside the open interval.
pub fn compaction_ratios_valid(limits: &wf_types::config::limits::CompactionLimits) -> bool {
    let h = limits.h_target_ratio.unwrap_or(H_TARGET_RATIO_DEFAULT);
    let ho = limits.t_handoff_ratio.unwrap_or(T_HANDOFF_RATIO_DEFAULT);
    let c = limits.t_compact_ratio.unwrap_or(T_COMPACT_RATIO_DEFAULT);
    let hard = limits.t_hard_ratio.unwrap_or(T_HARD_RATIO_DEFAULT);
    [h, ho, c, hard].iter().all(|r| *r > 0.0 && *r < 1.0) && h < ho && ho < c && c < hard
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::limits::CompactionLimits;

    fn limits() -> CompactionLimits {
        CompactionLimits {
            h_target_ratio: Some(0.30),
            t_handoff_ratio: Some(0.55),
            t_compact_ratio: Some(0.70),
            t_hard_ratio: Some(0.85),
            r_repress: None,
            r_min_range: None,
            k_repress: None,
            recent_raw_turns: None,
            tool_distill: None,
            file_dedup: None,
        }
    }

    #[test]
    fn thresholds_scale_with_window() {
        let t = resolve_compaction_thresholds(100_000, &limits());
        assert_eq!(
            t,
            CompactionThresholds {
                h_target: 30_000,
                t_handoff: 55_000,
                t_compact: 70_000,
                t_hard: 85_000,
            }
        );
    }

    #[test]
    fn zero_window_yields_zero_thresholds() {
        let t = resolve_compaction_thresholds(0, &limits());
        assert_eq!(t.h_target, 0);
        assert_eq!(t.t_hard, 0);
    }

    #[test]
    fn ratio_ordering_check() {
        assert!(compaction_ratios_valid(&limits()));
        let mut bad = limits();
        bad.t_compact_ratio = Some(0.50);
        assert!(!compaction_ratios_valid(&bad));
    }

    #[test]
    fn plan_item_clamps_to_snapshot() {
        let item = CompactionPlanItem {
            block_id: "b0001".to_string(),
            start: 2,
            end: 99,
            depth: 0,
        };
        assert_eq!(item.clamped_range(5), Some((2, 5)));
        let empty = CompactionPlanItem {
            block_id: "b0002".to_string(),
            start: 4,
            end: 4,
            depth: 1,
        };
        assert_eq!(empty.clamped_range(10), None);
    }
}
