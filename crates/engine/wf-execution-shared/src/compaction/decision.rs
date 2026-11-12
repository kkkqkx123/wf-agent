use wf_types::config::limits::CompactionLimits;
use wf_types::message::{BlockState, CompactionMode, CompactionState, CompressionBlock};

use super::budget::{k_repress, r_min_range, r_repress, resolve_compaction_thresholds};
use super::strategy::{compaction_strategy_name, is_single_summary_strategy};

/// Handoff reason: the hard limit still holds after aggressive compaction.
pub const REASON_HARD_LIMIT: &str = "HARD_LIMIT";
/// Handoff reason: still high after compaction with no compressible target.
pub const REASON_NO_COMPRESSIBLE_TARGET: &str = "NO_COMPRESSIBLE_TARGET";
/// Handoff reason: every active block is small, hot still exceeds budget.
pub const REASON_ALL_BLOCKS_SMALL: &str = "ALL_BLOCKS_SMALL";
/// Handoff reason: a still-over-budget completion landed in `Normal` mode.
pub const REASON_POST_COMPACT_STILL_HIGH: &str = "POST_COMPACT_STILL_HIGH";

/// Compaction decision for one settlement point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionDecision {
    Hold,
    Compact { aggressive: bool },
    Handoff { reason: &'static str },
    DropOldestActive,
}

/// Decide the next compaction step. Order: hard-limit compaction, compact
/// line, then handoff evaluation. `Handoff` mode only keeps the hard-limit
/// guard; the caller projects aggressive plans and converts a still-high
/// projection into `Handoff { HARD_LIMIT }`.
pub fn decide_compaction(
    hot_tokens: u64,
    state: &CompactionState,
    limits: &CompactionLimits,
) -> CompactionDecision {
    if state.window_tokens == 0 {
        return CompactionDecision::Hold;
    }
    let thresholds = resolve_compaction_thresholds(state.window_tokens, limits);
    if state.mode == CompactionMode::Handoff {
        if hot_tokens >= thresholds.t_hard && state.blocks.iter().any(|b| b.active) {
            return CompactionDecision::DropOldestActive;
        }
        return CompactionDecision::Hold;
    }
    if hot_tokens >= thresholds.t_hard {
        return CompactionDecision::Compact { aggressive: true };
    }
    if hot_tokens >= thresholds.t_compact {
        return CompactionDecision::Compact { aggressive: false };
    }
    // Single-summary mode gates only on thresholds: no block-based
    // handoff applies without a block table.
    if is_single_summary_strategy(&compaction_strategy_name(limits)) {
        return CompactionDecision::Hold;
    }
    let r_min = r_min_range(limits);
    let r_rep = r_repress(limits);
    let k_rep = k_repress(limits);
    let has_raw = state
        .blocks
        .iter()
        .any(|b| b.active && b.state == BlockState::Raw && b.size_tokens >= r_min);
    let has_repress = state.blocks.iter().any(|b| {
        b.active
            && b.state == BlockState::Compressed
            && b.size_tokens > r_rep
            && b.repress_count < k_rep
    });
    let over = hot_tokens > thresholds.h_target.saturating_add(thresholds.h_target / 2);
    // Handoff needs something to hand off: with no blocks yet the next
    // settlement syncs history into raw blocks and the compact line fires.
    if over && !state.blocks.is_empty() {
        let actives: Vec<&CompressionBlock> = state.blocks.iter().filter(|b| b.active).collect();
        let all_small = !actives.is_empty()
            && actives.iter().all(|b| {
                (b.state == BlockState::Compressed || b.state == BlockState::Sealed)
                    && b.size_tokens <= r_rep
            });
        if all_small {
            return CompactionDecision::Handoff {
                reason: REASON_ALL_BLOCKS_SMALL,
            };
        }
        if !has_raw && !has_repress {
            return CompactionDecision::Handoff {
                reason: REASON_NO_COMPRESSIBLE_TARGET,
            };
        }
    }
    CompactionDecision::Hold
}
