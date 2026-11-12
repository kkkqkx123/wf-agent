pub mod blocks;
pub mod budget;
pub mod decision;
pub mod plan;
pub mod strategy;

#[cfg(test)]
mod tests;

pub use blocks::{rebuild_after_truncate, sync_blocks};
pub use budget::{
    builtin_compaction_limits, compaction_ratios_valid, context_budget_from_profile,
    context_budget_from_window, context_budget_from_window_with_percent,
    context_budget_percent_from_metadata, resolve_compaction_thresholds,
    resolve_summary_call_budget, stateless_plan_factor, CONTEXT_BUDGET_METADATA_KEY,
    CONTEXT_BUDGET_PERCENT, H_TARGET_RATIO_DEFAULT, K_REPRESS_DEFAULT, PLAN_MAX_ITEMS_DEFAULT,
    RECENT_RAW_TURNS_DEFAULT, R_MIN_RANGE_DEFAULT, R_REPRESS_DEFAULT, SUMMARY_CALL_BUDGET_DEFAULT,
    SUMMARY_CALL_BUDGET_MAX, SUMMARY_CALL_BUDGET_MIN, T_COMPACT_RATIO_DEFAULT,
    T_HANDOFF_RATIO_DEFAULT, T_HARD_RATIO_DEFAULT,
};
pub use decision::{
    decide_compaction, CompactionDecision, REASON_ALL_BLOCKS_SMALL, REASON_HARD_LIMIT,
    REASON_NO_COMPRESSIBLE_TARGET, REASON_POST_COMPACT_STILL_HIGH,
};
pub use plan::{
    build_plan, build_stateless_plan, outcomes_match_plan, plan_covers_snapshot,
    stateless_plan_for_estimate, stateless_plan_for_workflow, CompactionPlan,
    DIRECTIVE_MAX_CHARS, DIRECTIVE_MAX_EST_TOKENS,
};
pub use strategy::{
    compaction_strategy_name, get_compaction_advisor, is_builtin_strategy,
    is_single_summary_strategy, process_compaction_limits, register_compaction_advisor,
    resolve_effective_compaction_limits, set_process_compaction_limits, strategy_workflow_ids,
    unregister_compaction_advisor, ADVISOR_PROTOCOL_VERSION,
    BUILTIN_COMPRESSION_STRATEGY_NAME, SINGLE_SUMMARY_STRATEGY_NAME,
};
pub use wf_types::message::{
    BlockOutcome, CompactionMode, CompactionPlanItem, CompactionThresholds, FileReadEntry,
};
