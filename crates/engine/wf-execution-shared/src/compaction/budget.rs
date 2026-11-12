use wf_types::config::limits::CompactionLimits;
use wf_types::message::CompactionThresholds;

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
/// Maximum summary-chain calls in one compaction round.
pub const PLAN_MAX_ITEMS_DEFAULT: usize = 8;
/// Fallback single-call budget when no model window is known.
pub const SUMMARY_CALL_BUDGET_DEFAULT: u64 = 16_000;
/// Bounds for the window-derived single-call budget.
pub const SUMMARY_CALL_BUDGET_MIN: u64 = 8_000;
pub const SUMMARY_CALL_BUDGET_MAX: u64 = 64_000;

/// Share of a model's context window usable for a single request input.
/// The remainder reserves headroom for the model output and local
/// estimation error. Single home for every context-budget derivation;
/// the tracker only stores the resolved limit.
/// Per-profile overrides live in the profile metadata map under
/// [`CONTEXT_BUDGET_METADATA_KEY`] so no struct change is needed.
pub const CONTEXT_BUDGET_PERCENT: u64 = 85;

/// Profile metadata key carrying a per-model budget percent override
/// (integer 1-100). Values outside the range fall back to the default.
pub const CONTEXT_BUDGET_METADATA_KEY: &str = "context_budget_percent";

/// Resolve the effective budget percent from a profile metadata map.
pub fn context_budget_percent_from_metadata(
    metadata: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> u64 {
    metadata
        .and_then(|m| m.get(CONTEXT_BUDGET_METADATA_KEY))
        .and_then(|v| v.as_u64())
        .filter(|p| (1..=100).contains(p))
        .unwrap_or(CONTEXT_BUDGET_PERCENT)
}

/// Derive the per-request context budget from a model window size.
/// Returns 0 (compression disabled) when the window is absent.
pub fn context_budget_from_window(window: Option<u32>) -> u64 {
    context_budget_from_window_with_percent(window, CONTEXT_BUDGET_PERCENT)
}

/// Derive the per-request context budget with an explicit percent.
/// Returns 0 (compression disabled) when the window is absent.
pub fn context_budget_from_window_with_percent(window: Option<u32>, percent: u64) -> u64 {
    let percent = percent.clamp(1, 100);
    window.map(|w| u64::from(w) * percent / 100).unwrap_or(0)
}

/// Derive the per-request context budget from a window plus an optional
/// profile metadata map (per-model percent override).
pub fn context_budget_from_profile(
    window: Option<u32>,
    metadata: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> u64 {
    context_budget_from_window_with_percent(window, context_budget_percent_from_metadata(metadata))
}

/// Built-in compaction parameters; the config processor fills every field,
/// this is the fallback before bootstrap injects the merged config.
pub fn builtin_compaction_limits() -> CompactionLimits {
    CompactionLimits {
        h_target_ratio: Some(H_TARGET_RATIO_DEFAULT),
        t_handoff_ratio: Some(T_HANDOFF_RATIO_DEFAULT),
        t_compact_ratio: Some(T_COMPACT_RATIO_DEFAULT),
        t_hard_ratio: Some(T_HARD_RATIO_DEFAULT),
        r_repress: Some(R_REPRESS_DEFAULT),
        r_min_range: Some(R_MIN_RANGE_DEFAULT),
        k_repress: Some(K_REPRESS_DEFAULT),
        recent_raw_turns: Some(RECENT_RAW_TURNS_DEFAULT),
        tool_distill: Some(true),
        file_dedup: Some(true),
        plan_max_items: Some(PLAN_MAX_ITEMS_DEFAULT),
        summary_call_budget_tokens: None,
        strategy: None,
    }
}

/// Resolve absolute thresholds from a model window and the configured
/// ratios. Missing ratios fall back to the documented defaults; a zero
/// window yields zero thresholds (compaction disabled).
pub fn resolve_compaction_thresholds(
    window_tokens: u64,
    limits: &CompactionLimits,
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
pub fn compaction_ratios_valid(limits: &CompactionLimits) -> bool {
    let h = limits.h_target_ratio.unwrap_or(H_TARGET_RATIO_DEFAULT);
    let ho = limits.t_handoff_ratio.unwrap_or(T_HANDOFF_RATIO_DEFAULT);
    let c = limits.t_compact_ratio.unwrap_or(T_COMPACT_RATIO_DEFAULT);
    let hard = limits.t_hard_ratio.unwrap_or(T_HARD_RATIO_DEFAULT);
    [h, ho, c, hard].iter().all(|r| *r > 0.0 && *r < 1.0) && h < ho && ho < c && c < hard
}

pub(super) fn r_min_range(limits: &CompactionLimits) -> u64 {
    limits.r_min_range.unwrap_or(R_MIN_RANGE_DEFAULT)
}

pub(super) fn r_repress(limits: &CompactionLimits) -> u64 {
    limits.r_repress.unwrap_or(R_REPRESS_DEFAULT)
}

pub(super) fn k_repress(limits: &CompactionLimits) -> u32 {
    limits.k_repress.unwrap_or(K_REPRESS_DEFAULT)
}

/// Resolve the single summary-call budget: an explicit configured value
/// wins, otherwise one tenth of the model window clamped into a sane band.
/// Callers must provide a known window; unknown windows hold upstream and
/// never reach planning.
pub fn resolve_summary_call_budget(window_tokens: u64, limits: &CompactionLimits) -> u64 {
    if let Some(explicit) = limits.summary_call_budget_tokens {
        return explicit.max(1);
    }
    (window_tokens / 10).clamp(SUMMARY_CALL_BUDGET_MIN, SUMMARY_CALL_BUDGET_MAX)
}

/// Hard-line factor mapping a trigger budget to its stateless-plan line:
/// mirrors the `t_compact` → `t_hard` spread of the threshold ladder, so a
/// moderately over-budget array keeps the single summary while a far over
/// budget one splits into a chunked plan. Validation keeps the factor
/// above one; the fallback does too.
pub fn stateless_plan_factor(limits: &CompactionLimits) -> f64 {
    let hard = limits.t_hard_ratio.unwrap_or(T_HARD_RATIO_DEFAULT);
    let compact = limits.t_compact_ratio.unwrap_or(T_COMPACT_RATIO_DEFAULT);
    if compact > 0.0 && hard > compact {
        hard / compact
    } else {
        T_HARD_RATIO_DEFAULT / T_COMPACT_RATIO_DEFAULT
    }
}
