use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

use wf_types::config::limits::CompactionLimits;
use wf_types::message::{
    BlockState, CompactionState, CompressionBlock, Message, MessageView, SeqRange,
};

pub use wf_types::message::CompactionPlanItem;

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

/// Handoff reason: the hard limit still holds after aggressive compaction.
pub const REASON_HARD_LIMIT: &str = "HARD_LIMIT";
/// Handoff reason: still high after compaction with no compressible target.
pub const REASON_NO_COMPRESSIBLE_TARGET: &str = "NO_COMPRESSIBLE_TARGET";
/// Handoff reason: every active block is small, hot still exceeds budget.
pub const REASON_ALL_BLOCKS_SMALL: &str = "ALL_BLOCKS_SMALL";
/// Handoff reason: a still-over-budget completion landed in `Normal` mode.
pub const REASON_POST_COMPACT_STILL_HIGH: &str = "POST_COMPACT_STILL_HIGH";

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

/// Built-in compression strategy name. Empty config normalizes here.
pub const BUILTIN_COMPRESSION_STRATEGY_NAME: &str = "builtin";
/// Built-in single-summary mode name. Selects the legacy single summary
/// without layered blocks: thresholds still gate emission, but no block
/// planning, re-compression or block-based handoff applies.
pub const SINGLE_SUMMARY_STRATEGY_NAME: &str = "single-summary";

/// Effective strategy name for compaction limits. Trims the configured
/// value; absent or blank means the built-in strategy.
pub fn compaction_strategy_name(limits: &CompactionLimits) -> String {
    limits
        .strategy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(BUILTIN_COMPRESSION_STRATEGY_NAME)
        .to_string()
}

/// Whether a strategy name selects the built-in planner and chains.
pub fn is_builtin_strategy(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || trimmed == BUILTIN_COMPRESSION_STRATEGY_NAME
}

/// Whether a strategy name selects the legacy single summary without
/// layered blocks. The engine still owns thresholds, version anchors
/// and write-back; planning and block-based handoff are bypassed.
pub fn is_single_summary_strategy(name: &str) -> bool {
    name.trim() == SINGLE_SUMMARY_STRATEGY_NAME
}

/// Version of the advisor planning protocol. Sent with every advisor
/// input so future protocol evolution can branch on it.
pub const ADVISOR_PROTOCOL_VERSION: u32 = 1;

/// Candidate summary workflow ids for a strategy name, before registry
/// lookup. Built-in names keep the fold-summary chains; any other name
/// resolves to the strategy id with `-repress` suffixed for the
/// re-compression chain when present.
pub fn strategy_workflow_ids(strategy: &str) -> (String, Option<String>) {
    if is_builtin_strategy(strategy) || is_single_summary_strategy(strategy) {
        (
            wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID.to_string(),
            Some(wf_resource::predefined::workflow::FOLD_SUMMARY_REPRESS_WORKFLOW_ID.to_string()),
        )
    } else {
        let trimmed = strategy.trim().to_string();
        let repress = format!("{trimmed}-repress");
        (trimmed, Some(repress))
    }
}

/// Process-wide compaction advisor registry backing named strategies.
/// Advisors are pure shaping hooks contributed by plugins; the engine
/// validates every suggestion and falls back to the built-in planner.
static COMPACTION_ADVISORS: OnceLock<
    dashmap::DashMap<String, std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>>,
> = OnceLock::new();

fn compaction_advisors(
) -> &'static dashmap::DashMap<String, std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>> {
    COMPACTION_ADVISORS.get_or_init(dashmap::DashMap::new)
}

/// Register planning advice for a named strategy. Returns false when the
/// name is already taken; unregister first to replace it.
pub fn register_compaction_advisor(
    name: &str,
    advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>,
) -> bool {
    let key = name.trim().to_string();
    if key.is_empty() || compaction_advisors().contains_key(&key) {
        return false;
    }
    compaction_advisors().insert(key, advisor);
    true
}

/// Remove planning advice for a named strategy.
pub fn unregister_compaction_advisor(name: &str) -> bool {
    compaction_advisors().remove(name.trim()).is_some()
}

/// Planning advice for a named strategy, if registered.
pub fn get_compaction_advisor(
    name: &str,
) -> Option<std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>> {
    compaction_advisors().get(name.trim()).map(|e| e.clone())
}

static PROCESS_LIMITS: RwLock<Option<CompactionLimits>> = RwLock::new(None);

/// Inject the process-wide compaction parameters at bootstrap, mirroring
/// the compression settle-budget injection. Later installs overwrite
/// earlier ones so tests and re-bootstrap observe the newest config.
pub fn set_process_compaction_limits(limits: CompactionLimits) {
    *PROCESS_LIMITS
        .write()
        .expect("compaction limits lock poisoned") = Some(limits);
}

/// Effective process-wide compaction parameters. Bootstrap must install
/// them before any execution emits; missing installation fails loudly
/// instead of silently falling back to built-in defaults.
pub fn process_compaction_limits() -> CompactionLimits {
    PROCESS_LIMITS
        .read()
        .expect("compaction limits lock poisoned")
        .clone()
        .expect("compaction limits not installed")
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

fn r_min_range(limits: &CompactionLimits) -> u64 {
    limits.r_min_range.unwrap_or(R_MIN_RANGE_DEFAULT)
}

fn r_repress(limits: &CompactionLimits) -> u64 {
    limits.r_repress.unwrap_or(R_REPRESS_DEFAULT)
}

fn k_repress(limits: &CompactionLimits) -> u32 {
    limits.k_repress.unwrap_or(K_REPRESS_DEFAULT)
}

/// Sync history appended since the last sync into the trailing `Raw` block.
/// Estimates come from the same counter the view estimate uses, so block
/// sizes and the hot estimate stay on one scale. Freshness means nothing
/// was ever synced: an emptied block table (post-handoff or post-GC
/// collection) must not resurrect already-synced history.
pub fn sync_blocks(state: &mut CompactionState, messages: &[Message], seqs: &[u64]) {
    if messages.len() != seqs.len() || messages.is_empty() {
        return;
    }
    let fresh = state.blocks.is_empty() && state.synced_through_seq == 0;
    let known: HashSet<&str> = state
        .blocks
        .iter()
        .map(|b| b.summary_message_id.as_str())
        .filter(|id| !id.is_empty())
        .chain(
            state
                .handoff
                .as_ref()
                .map(|h| h.handoff_message_id.as_str()),
        )
        .collect();
    let mut spans: Vec<SeqRange> = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    let mut last_seq = state.synced_through_seq;
    for (message, seq) in messages.iter().zip(seqs.iter()) {
        if !fresh && *seq <= state.synced_through_seq {
            continue;
        }
        // Summaries belong to their blocks, never to the raw tail: the
        // write-back appends them through the normal append path.
        if known.contains(message.id.as_str()) {
            last_seq = last_seq.max(*seq);
            continue;
        }
        let size = wf_llm::count::estimate_message_tokens(message) as u64;
        match spans.last_mut() {
            Some(span) if span.end.saturating_add(1) == *seq => {
                span.end = *seq;
                if let Some(total) = sizes.last_mut() {
                    *total = total.saturating_add(size);
                }
            }
            _ => {
                spans.push(SeqRange {
                    start: *seq,
                    end: *seq,
                });
                sizes.push(size);
            }
        }
        last_seq = last_seq.max(*seq);
    }
    if spans.is_empty() {
        return;
    }
    let total: u64 = sizes.iter().sum();
    match state.blocks.last_mut() {
        Some(tail) if tail.state == BlockState::Raw && tail.active => {
            for span in spans {
                match tail.spans.last_mut() {
                    Some(existing) if existing.end.saturating_add(1) == span.start => {
                        existing.end = span.end;
                    }
                    _ => tail.spans.push(span),
                }
            }
            tail.size_tokens = tail.size_tokens.saturating_add(total);
            tail.source_tokens = tail.source_tokens.saturating_add(total);
        }
        _ => {
            let id = state.next_block_seq;
            state.next_block_seq = state.next_block_seq.saturating_add(1);
            state.blocks.push(CompressionBlock {
                block_id: format!("b{id:04}"),
                state: BlockState::Raw,
                topic: String::new(),
                summary_message_id: String::new(),
                spans,
                merged_blocks: Vec::new(),
                size_tokens: total,
                source_tokens: total,
                repress_count: 0,
                active: true,
                created_at: wf_common::now(),
                compressed_at: None,
            });
        }
    }
    state.synced_through_seq = last_seq;
}

/// Sequence coordinate per snapshot position (`None` for undecodable
/// positions). `Full` snapshots align with history; compacted snapshots
/// resolve summaries by id and ranges by interval.
fn snapshot_seq_map(
    view: &MessageView,
    snapshot: &[Message],
    history: &[Message],
    seqs: &[u64],
) -> Vec<Option<u64>> {
    if matches!(view, MessageView::Full) && snapshot.len() == history.len() {
        return seqs.iter().copied().map(Some).collect();
    }
    let id_to_seq: HashMap<&str, u64> = history
        .iter()
        .zip(seqs.iter())
        .map(|(m, s)| (m.id.as_str(), *s))
        .collect();
    match view {
        MessageView::Compacted { segments } => {
            let mut seqs_out = Vec::with_capacity(snapshot.len());
            for segment in segments {
                match segment {
                    wf_types::message::ViewSegment::Summary { message_id } => {
                        seqs_out.push(id_to_seq.get(message_id.as_str()).copied());
                    }
                    wf_types::message::ViewSegment::Range { start_seq, end_seq } => {
                        for seq in seqs.iter() {
                            if *seq >= *start_seq && *seq <= *end_seq {
                                seqs_out.push(Some(*seq));
                            }
                        }
                    }
                }
            }
            seqs_out.truncate(snapshot.len());
            while seqs_out.len() < snapshot.len() {
                seqs_out.push(None);
            }
            seqs_out
        }
        _ => snapshot.iter().map(|_| None).collect(),
    }
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

/// Build a one-shot stateless plan over snapshot costs: chunk into
/// call-budget slices with temporary `w`-prefixed block ids, then widen
/// adjacent slices while the item count exceeds `max_items`. Coverage is
/// never truncated (every index belongs to exactly one item); the widening
/// loop only fires on extreme over-budget arrays. No block table is
/// involved: the pipeline aggregates the item summaries and reports the
/// temporary ids back as outcomes.
pub fn build_stateless_plan(
    costs: &[u64],
    budget: u64,
    max_items: usize,
) -> Vec<CompactionPlanItem> {
    let budget = budget.max(1);
    let cap = max_items.max(1);
    let mut items = Vec::new();
    let mut cursor = 0usize;
    while cursor < costs.len() && items.len() < cap {
        let mut end = cursor;
        let mut acc = 0u64;
        while end < costs.len() {
            if acc > 0 && acc.saturating_add(costs[end]) > budget {
                break;
            }
            acc = acc.saturating_add(costs[end]);
            end = end.saturating_add(1);
        }
        if end == cursor {
            end = end.saturating_add(1);
        }
        items.push(CompactionPlanItem {
            block_id: format!("w{0:04}", items.len()),
            start: cursor,
            end,
            depth: 0,
            extra_block_ids: Vec::new(),
        });
        cursor = end;
    }
    // Uncovered tail (cap hit before the end): append it whole so coverage
    // stays complete, then widen until the cap holds again.
    if cursor < costs.len() {
        items.push(CompactionPlanItem {
            block_id: format!("w{0:04}", items.len()),
            start: cursor,
            end: costs.len(),
            depth: 0,
            extra_block_ids: Vec::new(),
        });
    }
    while items.len() > cap {
        let mut best = 0usize;
        let mut best_cost = u64::MAX;
        for i in 0..items.len().saturating_sub(1) {
            let combined: u64 = (items[i].start..items[i + 1].end)
                .map(|idx| costs.get(idx).copied().unwrap_or(0))
                .sum();
            if combined < best_cost {
                best_cost = combined;
                best = i;
            }
        }
        let end = items[best + 1].end;
        items[best].end = end;
        items.remove(best + 1);
    }
    items
}

/// Shared stateless planning for far over-budget snapshots without a block
/// table: moderately over-budget arrays keep the single summary, far over
/// budget ones split into call-budget chunks. Single-summary strategy never
/// upgrades. Unknown budgets stay single-summary upstream.
/// Empty return means the legacy single-summary mode.
pub fn stateless_plan_for_estimate(
    messages: &[Message],
    estimated: u64,
    context_limit: u64,
    limits: &CompactionLimits,
) -> Vec<CompactionPlanItem> {
    if context_limit == 0 {
        return Vec::new();
    }
    if is_single_summary_strategy(&compaction_strategy_name(limits)) {
        return Vec::new();
    }
    let hard_line = (context_limit as f64 * stateless_plan_factor(limits)) as u64;
    if estimated <= hard_line {
        return Vec::new();
    }
    let budget = resolve_summary_call_budget(context_limit, limits);
    let cap = plan_max_items(limits);
    let costs: Vec<u64> = messages
        .iter()
        .map(|message| wf_llm::count::estimate_message_tokens(message) as u64)
        .collect();
    build_stateless_plan(&costs, budget, cap)
}

/// Maximum summary-chain calls in one compaction round.
fn plan_max_items(limits: &CompactionLimits) -> usize {
    limits
        .plan_max_items
        .unwrap_or(PLAN_MAX_ITEMS_DEFAULT)
        .max(1)
}

/// Consult the registered planning advice for the effective strategy. A
/// registered advisor enhances whatever strategy is active, including the
/// built-in one; only a missing advisor, a failure or an invalid
/// suggestion falls back to the built-in planner. Returns the validated
/// plan when the suggestion survives engine checks, `None` otherwise.
fn try_advisor_plan(
    state: &CompactionState,
    snapshot: &[Message],
    hot_tokens: u64,
    limits: &CompactionLimits,
    aggressive: bool,
) -> Option<CompactionPlan> {
    let strategy = compaction_strategy_name(limits);
    let Some(advisor) = get_compaction_advisor(&strategy) else {
        tracing::debug!(strategy = %strategy, "compaction advisor missing; falling back to built-in plan");
        return None;
    };
    try_advisor_plan_with(
        &strategy, &advisor, state, snapshot, hot_tokens, limits, aggressive,
    )
}

/// Validate one advisor suggestion against the live block table and the
/// snapshot bounds. Split from [`try_advisor_plan`] so the validation is
/// testable without touching the process-wide advisor registry: unknown
/// block ids, out-of-range indices and over-cap suggestions never reach
/// execution, and an empty suggestion defers to the built-in planner.
fn try_advisor_plan_with(
    strategy: &str,
    advisor: &std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>,
    state: &CompactionState,
    snapshot: &[Message],
    hot_tokens: u64,
    limits: &CompactionLimits,
    aggressive: bool,
) -> Option<CompactionPlan> {
    let thresholds = resolve_compaction_thresholds(state.window_tokens, limits);
    let target_freed = hot_tokens.saturating_sub(thresholds.h_target);
    let item_cap = plan_max_items(limits);
    let costs: Vec<u64> = snapshot
        .iter()
        .map(|message| wf_llm::count::estimate_message_tokens(message) as u64)
        .collect();
    let blocks: Vec<serde_json::Value> = state
        .blocks
        .iter()
        .filter(|b| b.active)
        .map(|b| {
            serde_json::json!({
                "block_id": b.block_id,
                "state": format!("{:?}", b.state),
                "size_tokens": b.size_tokens,
                "repress_count": b.repress_count,
            })
        })
        .collect();
    let input = serde_json::json!({
        "protocol_version": ADVISOR_PROTOCOL_VERSION,
        "strategy": strategy,
        "hot_tokens": hot_tokens,
        "aggressive": aggressive,
        "item_cap": item_cap,
        "target_freed": target_freed,
        "snapshot_len": snapshot.len(),
        "costs": costs,
        "blocks": blocks,
        "thresholds": {
            "h_target": thresholds.h_target,
            "t_handoff": thresholds.t_handoff,
            "t_compact": thresholds.t_compact,
            "t_hard": thresholds.t_hard,
        },
    });
    let output = match advisor.advise(input) {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(strategy = %strategy, error = %error, "compaction advisor failed; falling back to built-in plan");
            return None;
        }
    };
    // Explicit protocol negotiation: advisors that declare a version must
    // speak the engine's; silence keeps older advisors working, mismatch
    // rejects loudly instead of misreading new fields as old ones.
    if let Some(version) = output.get("protocol_version").and_then(|v| v.as_u64()) {
        if version != u64::from(ADVISOR_PROTOCOL_VERSION) {
            tracing::warn!(strategy = %strategy, version, "compaction advisor protocol mismatch; falling back to built-in plan");
            return None;
        }
    }
    let mut plan = CompactionPlan::empty();
    let known: std::collections::HashSet<&str> = state
        .blocks
        .iter()
        .filter(|b| b.active)
        .map(|b| b.block_id.as_str())
        .collect();
    let raw_ids: std::collections::HashSet<&str> = state
        .blocks
        .iter()
        .filter(|b| b.active && b.state == BlockState::Raw)
        .map(|b| b.block_id.as_str())
        .collect();
    if let Some(items) = output.get("items").and_then(|v| v.as_array()) {
        for item in items {
            if plan.items.len() >= item_cap {
                break;
            }
            let block_id = item.get("block_id").and_then(|v| v.as_str()).unwrap_or("");
            if block_id.is_empty() || !known.contains(block_id) {
                continue;
            }
            let start = item
                .get("start")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX) as usize;
            let end = item.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if start >= end || end > snapshot.len() {
                continue;
            }
            let depth = item.get("depth").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let mut extras = Vec::new();
            if let Some(extra_ids) = item.get("extra_block_ids").and_then(|v| v.as_array()) {
                for extra in extra_ids {
                    if let Some(id) = extra.as_str() {
                        if !id.is_empty()
                            && known.contains(id)
                            && id != block_id
                            && !extras.contains(&id.to_string())
                        {
                            extras.push(id.to_string());
                        }
                    }
                }
            }
            let covered: u64 = (start..end)
                .map(|i| costs.get(i).copied().unwrap_or(0))
                .sum();
            plan.freed_tokens = plan.freed_tokens.saturating_add(covered);
            plan.items.push(CompactionPlanItem {
                block_id: block_id.to_string(),
                start,
                end,
                depth,
                extra_block_ids: extras,
            });
        }
    }
    if let Some(drops) = output.get("drops").and_then(|v| v.as_array()) {
        for drop in drops {
            if let Some(id) = drop.as_str() {
                if id.is_empty() || raw_ids.contains(id) || plan.drops.iter().any(|d| d == id) {
                    continue;
                }
                if !known.contains(id) {
                    continue;
                }
                if let Some(block) = state.blocks.iter().find(|b| b.block_id == id) {
                    plan.freed_tokens = plan.freed_tokens.saturating_add(block.size_tokens);
                    plan.drops.push(id.to_string());
                }
            }
        }
    }
    match sanitize_directive(output.get("directive").and_then(|v| v.as_str())) {
        Ok(directive) => plan.directive = directive,
        Err(()) => {
            tracing::warn!(strategy = %strategy, "compaction advisor directive over limit; falling back to built-in plan");
            return None;
        }
    }
    if plan.is_empty() {
        return None;
    }
    plan.capped = plan.items.len() >= item_cap && plan.freed_tokens < target_freed;
    Some(plan)
}

/// Merge adjacent first-compression items in snapshot order while the
/// merged slice fits the single-call budget. Only `depth == 0` items
/// merge; re-compressions keep their identity. Merged coverage beyond the
/// primary block travels in `extra_block_ids` so the service reports one
/// outcome per covered block.
fn merge_adjacent_items(items: &mut Vec<CompactionPlanItem>, costs: &[u64], budget: u64) {
    if items.len() < 2 {
        return;
    }
    items.sort_by_key(|item| item.start);
    let mut merged: Vec<CompactionPlanItem> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        if item.depth != 0 {
            merged.push(item);
            continue;
        }
        let adjacent = merged.last_mut().filter(|prev| {
            prev.depth == 0 && prev.end == item.start && item.extra_block_ids.is_empty()
        });
        if let Some(prev) = adjacent {
            let prev_cost: u64 = (prev.start..prev.end)
                .map(|i| costs.get(i).copied().unwrap_or(0))
                .sum();
            let item_cost: u64 = (item.start..item.end)
                .map(|i| costs.get(i).copied().unwrap_or(0))
                .sum();
            if prev_cost.saturating_add(item_cost) <= budget
                && !prev.extra_block_ids.contains(&item.block_id)
                && prev.block_id != item.block_id
            {
                prev.end = item.end;
                prev.extra_block_ids.push(item.block_id);
                continue;
            }
        }
        merged.push(item);
    }
    *items = merged;
}

/// Maximum request-level directive length from a planning suggestion:
/// 2000 characters with roughly 500 estimated tokens, dual-constrained.
/// Sized against the summary-input headroom (the pipeline trims history
/// to 90% of the emission budget while the smallest single-call budget is
/// 8000 tokens): a capped directive always fits the headroom with
/// estimation error to spare, so it never eats the history budget.
/// Over either limit rejects the whole suggestion and the engine falls
/// back to the built-in plan.
pub const DIRECTIVE_MAX_CHARS: usize = 2000;
pub const DIRECTIVE_MAX_EST_TOKENS: u64 = 500;

/// Sanitize a suggested request-level directive: trim, drop empties, and
/// reject over-long values under the dual constraint. `Ok(None)` means no
/// directive was suggested; `Err` means one was suggested but over-long,
/// and the caller must reject the whole suggestion for the built-in plan.
fn sanitize_directive(raw: Option<&str>) -> Result<Option<String>, ()> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if trimmed.chars().count() > DIRECTIVE_MAX_CHARS
        || wf_llm::estimate_tokens(trimmed) as u64 > DIRECTIVE_MAX_EST_TOKENS
    {
        return Err(());
    }
    Ok(Some(trimmed.to_string()))
}

/// One compaction emission: work items plus block ids to deactivate on
/// success, plus the freed token estimate the decision uses.
pub struct CompactionPlan {
    pub items: Vec<CompactionPlanItem>,
    pub drops: Vec<String>,
    pub freed_tokens: u64,
    /// True when the per-round item cap stopped construction while the
    /// reclaim target was still unmet (the remainder waits for the next
    /// settlement round).
    pub capped: bool,
    /// Request-level instruction from the planning suggestion, carried by
    /// every slice of this emission. Never stored in the pending plan;
    /// emitters move it into the signal request.
    pub directive: Option<String>,
}

impl CompactionPlan {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            drops: Vec::new(),
            freed_tokens: 0,
            capped: false,
            directive: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.drops.is_empty()
    }
}

/// Build the compaction plan for the current hot size: largest `Raw` blocks
/// first until the hot size projects back to target, then re-compressible
/// summaries, then smallest compressed blocks for deactivation. `Raw`
/// blocks are never deactivated. Aggressive mode (past the hard limit)
/// ignores the minimum interval and deactivates small blocks too.
/// Registered planning advice is consulted first for every strategy (a
/// `builtin` advisor enhances the built-in planner); any missing, failing
/// or invalid advice falls back to the built-in planner.
#[allow(clippy::too_many_arguments)]
pub fn build_plan(
    state: &CompactionState,
    view: &MessageView,
    snapshot: &[Message],
    history: &[Message],
    seqs: &[u64],
    hot_tokens: u64,
    limits: &CompactionLimits,
    aggressive: bool,
) -> CompactionPlan {
    let thresholds = resolve_compaction_thresholds(state.window_tokens, limits);
    if state.window_tokens == 0 || hot_tokens <= thresholds.h_target {
        return CompactionPlan::empty();
    }
    // Single-summary mode keeps no layered plan: the empty plan is the
    // legacy single-summary signal, validated by the emitter holding a
    // Compact decision.
    if is_single_summary_strategy(&compaction_strategy_name(limits)) {
        return CompactionPlan::empty();
    }
    if let Some(advised) = try_advisor_plan(state, snapshot, hot_tokens, limits, aggressive) {
        return advised;
    }
    let r_min = if aggressive { 0 } else { r_min_range(limits) };
    let r_rep = r_repress(limits);
    let k_rep = k_repress(limits);
    let target_freed = hot_tokens.saturating_sub(thresholds.h_target);
    let call_budget = resolve_summary_call_budget(state.window_tokens, limits);
    let item_cap = plan_max_items(limits);
    let costs: Vec<u64> = snapshot
        .iter()
        .map(|message| wf_llm::count::estimate_message_tokens(message) as u64)
        .collect();
    let snap_seqs = snapshot_seq_map(view, snapshot, history, seqs);
    let present: HashSet<u64> = snap_seqs.iter().filter_map(|s| *s).collect();
    let mut present_index: HashMap<u64, usize> = HashMap::new();
    for (index, seq) in snap_seqs.iter().enumerate() {
        if let Some(seq) = seq {
            present_index.entry(*seq).or_insert(index);
        }
    }
    let mut plan = CompactionPlan::empty();
    let mut raws: Vec<&CompressionBlock> = state
        .blocks
        .iter()
        .filter(|b| {
            b.active
                && b.state == BlockState::Raw
                && b.size_tokens >= r_min
                && b.spans
                    .iter()
                    .flat_map(|span| span.start..=span.end)
                    .all(|seq| present.contains(&seq))
        })
        .collect();
    raws.sort_by_key(|b| std::cmp::Reverse(b.size_tokens));
    for block in raws {
        if plan.freed_tokens >= target_freed || plan.items.len() >= item_cap {
            break;
        }
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        for span in &block.spans {
            for seq in span.start..=span.end {
                if let Some(index) = present_index.get(&seq) {
                    lo = lo.min(*index);
                    hi = hi.max(*index);
                }
            }
        }
        if lo == usize::MAX {
            continue;
        }
        plan.freed_tokens = plan.freed_tokens.saturating_add(block.size_tokens);
        // Split oversized blocks against the single-call budget so one
        // item always fits a summary call; a lone oversized message
        // still progresses on its own.
        let mut cursor = lo;
        while cursor <= hi && plan.items.len() < item_cap {
            let mut end = cursor;
            let mut acc = 0u64;
            while end <= hi {
                let cost = costs.get(end).copied().unwrap_or(0);
                if acc > 0 && acc.saturating_add(cost) > call_budget {
                    break;
                }
                acc = acc.saturating_add(cost);
                end = end.saturating_add(1);
            }
            if end == cursor {
                end = end.saturating_add(1);
            }
            plan.items.push(CompactionPlanItem {
                block_id: block.block_id.clone(),
                start: cursor,
                end,
                depth: 0,
                extra_block_ids: Vec::new(),
            });
            cursor = end;
        }
    }
    merge_adjacent_items(&mut plan.items, &costs, call_budget);
    if plan.freed_tokens < target_freed {
        let mut repress: Vec<&CompressionBlock> = state
            .blocks
            .iter()
            .filter(|b| {
                b.active
                    && b.state == BlockState::Compressed
                    && b.size_tokens > r_rep
                    && b.repress_count < k_rep
                    && !b.summary_message_id.is_empty()
                    && snapshot.iter().any(|m| m.id == b.summary_message_id)
            })
            .collect();
        repress.sort_by_key(|b| std::cmp::Reverse(b.size_tokens));
        for block in repress {
            if plan.freed_tokens >= target_freed || plan.items.len() >= item_cap {
                break;
            }
            let index = snapshot
                .iter()
                .position(|m| m.id == block.summary_message_id);
            let Some(index) = index else { continue };
            plan.freed_tokens = plan.freed_tokens.saturating_add(block.size_tokens / 2);
            plan.items.push(CompactionPlanItem {
                block_id: block.block_id.clone(),
                start: index,
                end: index.saturating_add(1),
                depth: block.repress_count.saturating_add(1),
                extra_block_ids: Vec::new(),
            });
        }
    }
    if plan.freed_tokens < target_freed {
        let mut small: Vec<&CompressionBlock> = state
            .blocks
            .iter()
            .filter(|b| {
                b.active && (b.state == BlockState::Compressed || b.state == BlockState::Sealed)
            })
            .collect();
        small.sort_by_key(|b| b.size_tokens);
        for block in small {
            if plan.freed_tokens >= target_freed {
                break;
            }
            plan.freed_tokens = plan.freed_tokens.saturating_add(block.size_tokens);
            plan.drops.push(block.block_id.clone());
        }
    }
    plan.capped = plan.items.len() >= item_cap && plan.freed_tokens < target_freed;
    plan
}

/// Whether a plan covers its snapshot: every item names a non-empty block
/// and stays inside the snapshot bounds. Pure validator for the string-id
/// contract between emitters, templates and services.
pub fn plan_covers_snapshot(plan: &CompactionPlan, snapshot_len: usize) -> bool {
    plan.items
        .iter()
        .all(|item| !item.block_id.is_empty() && item.start < item.end && item.end <= snapshot_len)
}

/// Whether every outcome names a block from the pending plan. An empty
/// plan means the legacy single-summary path with no block contract, so it
/// matches any outcome batch.
pub fn outcomes_match_plan(outcomes: &[BlockOutcome], plan: &[CompactionPlanItem]) -> bool {
    if plan.is_empty() {
        return true;
    }
    outcomes
        .iter()
        .all(|outcome| plan.iter().any(|item| item.block_id == outcome.block_id))
}

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

/// Rebuild block bookkeeping after a branch truncation: drop blocks whose
/// spans cross the cut, downgrade summaries lost with the cut back to
/// `Raw`, forget file reads past the cut, and clear a handoff whose
/// document is gone. `history`/`seqs` are the truncated prefix.
pub fn rebuild_after_truncate(state: &mut CompactionState, history: &[Message], seqs: &[u64]) {
    let target = seqs.last().copied().unwrap_or(0);
    state.blocks.retain(|b| {
        b.spans
            .iter()
            .all(|span| span.start <= target && span.end <= target)
    });
    let present: HashSet<&str> = history.iter().map(|m| m.id.as_str()).collect();
    for block in state.blocks.iter_mut() {
        if !block.summary_message_id.is_empty()
            && !present.contains(block.summary_message_id.as_str())
        {
            block.state = BlockState::Raw;
            block.summary_message_id.clear();
            block.repress_count = 0;
            block.compressed_at = None;
        }
    }
    state
        .file_reads
        .retain(|_, entry| entry.message_seq <= target);
    if let Some(handoff) = state.handoff.as_ref() {
        if !present.contains(handoff.handoff_message_id.as_str()) {
            state.handoff = None;
            state.mode = CompactionMode::Normal;
        }
    }
    state.synced_through_seq = state.synced_through_seq.min(target);
    state.pending_plan.clear();
    state.pending_drops.clear();
    state.dirty = true;
}

/// Ingest shaping lives in [`crate::ingest_preprocess`]; compaction keeps
/// only budget, plan and block bookkeeping and never touches raw tool text.
/// Re-export outcome and entry types for event, session and ingest layers.
pub use wf_types::message::{BlockOutcome, CompactionMode, CompactionThresholds, FileReadEntry};

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
            plan_max_items: None,
            summary_call_budget_tokens: None,
            strategy: None,
        }
    }

    #[test]
    fn thresholds_scale_with_window() {
        let t = resolve_compaction_thresholds(100_000, &limits());
        assert_eq!(t.h_target, 30_000);
        assert_eq!(t.t_handoff, 55_000);
        assert_eq!(t.t_compact, 70_000);
        assert_eq!(t.t_hard, 85_000);
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
            extra_block_ids: Vec::new(),
        };
        assert_eq!(item.clamped_range(5), Some((2, 5)));
        let empty = CompactionPlanItem {
            block_id: "b0002".to_string(),
            start: 4,
            end: 4,
            depth: 1,
            extra_block_ids: Vec::new(),
        };
        assert_eq!(empty.clamped_range(5), None);
    }

    #[test]
    fn small_blocks_hold() {
        let state = CompactionState {
            window_tokens: 100_000,
            thresholds: resolve_compaction_thresholds(100_000, &limits()),
            ..Default::default()
        };

        assert_eq!(
            decide_compaction(10_000, &state, &limits()),
            CompactionDecision::Hold
        );
        assert_eq!(
            decide_compaction(69_999, &state, &limits()),
            CompactionDecision::Hold
        );
    }

    #[test]
    fn compact_line_triggers() {
        let state = CompactionState {
            window_tokens: 100_000,
            thresholds: resolve_compaction_thresholds(100_000, &limits()),
            ..Default::default()
        };

        assert_eq!(
            decide_compaction(70_000, &state, &limits()),
            CompactionDecision::Compact { aggressive: false }
        );
        assert_eq!(
            decide_compaction(85_000, &state, &limits()),
            CompactionDecision::Compact { aggressive: true }
        );
    }

    #[test]
    fn all_small_blocks_handoff() {
        let mut state = CompactionState {
            window_tokens: 100_000,
            thresholds: resolve_compaction_thresholds(100_000, &limits()),
            ..Default::default()
        };

        state.blocks.push(CompressionBlock {
            block_id: "b0000".to_string(),
            state: BlockState::Compressed,
            topic: String::new(),
            summary_message_id: "s1".to_string(),
            spans: vec![SeqRange { start: 0, end: 5 }],
            merged_blocks: Vec::new(),
            size_tokens: 1000,
            source_tokens: 8000,
            repress_count: 2,
            active: true,
            created_at: 0,
            compressed_at: None,
        });
        assert_eq!(
            decide_compaction(46_000, &state, &limits()),
            CompactionDecision::Handoff {
                reason: REASON_ALL_BLOCKS_SMALL
            }
        );
    }

    fn plan_message(text: &str) -> Message {
        Message::user_text(text.to_string())
    }

    fn raw_block(id: &str, start: u64, end: u64, size: u64) -> CompressionBlock {
        CompressionBlock {
            block_id: id.to_string(),
            state: BlockState::Raw,
            topic: String::new(),
            summary_message_id: String::new(),
            spans: vec![SeqRange { start, end }],
            merged_blocks: Vec::new(),
            size_tokens: size,
            source_tokens: size,
            repress_count: 0,
            active: true,
            created_at: 0,
            compressed_at: None,
        }
    }

    fn plan_state(blocks: Vec<CompressionBlock>) -> CompactionState {
        CompactionState {
            window_tokens: 100_000,
            thresholds: resolve_compaction_thresholds(100_000, &limits()),
            blocks,
            ..Default::default()
        }
    }

    #[test]
    fn call_budget_derives_from_window() {
        assert_eq!(resolve_summary_call_budget(100_000, &limits()), 10_000);
        assert_eq!(
            resolve_summary_call_budget(0, &limits()),
            SUMMARY_CALL_BUDGET_MIN
        );
        let mut explicit = limits();
        explicit.summary_call_budget_tokens = Some(3000);
        assert_eq!(resolve_summary_call_budget(100_000, &explicit), 3000);
    }

    #[test]
    fn plan_stops_at_item_cap() {
        let messages: Vec<Message> = (0..14)
            .map(|i| plan_message(&format!("pad message {i}")))
            .collect();
        let seqs: Vec<u64> = (0..14).collect();
        let blocks = vec![
            raw_block("b0000", 0, 1, 20_000),
            raw_block("b0001", 4, 5, 20_000),
            raw_block("b0002", 8, 9, 20_000),
            raw_block("b0003", 12, 13, 20_000),
        ];
        let mut capped = limits();
        capped.plan_max_items = Some(2);
        let plan = build_plan(
            &plan_state(blocks),
            &MessageView::Full,
            &messages,
            &messages,
            &seqs,
            90_000,
            &capped,
            false,
        );
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].block_id, "b0000");
        assert_eq!(plan.items[1].block_id, "b0001");
    }

    #[test]
    fn oversized_block_splits_against_call_budget() {
        let messages = vec![
            plan_message("aaa"),
            plan_message("bbb"),
            plan_message("ccc"),
        ];
        let costs: Vec<u64> = messages
            .iter()
            .map(|m| wf_llm::count::estimate_message_tokens(m) as u64)
            .collect();
        let mut budgeted = limits();
        budgeted.summary_call_budget_tokens = Some(costs[0].saturating_add(costs[1]).max(1));
        budgeted.plan_max_items = Some(8);
        let seqs = vec![0, 1, 2];
        let plan = build_plan(
            &plan_state(vec![raw_block("b0000", 0, 2, 20_000)]),
            &MessageView::Full,
            &messages,
            &messages,
            &seqs,
            90_000,
            &budgeted,
            false,
        );
        assert_eq!(plan.items.len(), 2);
        assert_eq!((plan.items[0].start, plan.items[0].end), (0, 2));
        assert_eq!((plan.items[1].start, plan.items[1].end), (2, 3));
        assert!(plan.items.iter().all(|item| item.block_id == "b0000"));
        assert_eq!(plan.freed_tokens, 20_000);
    }

    #[test]
    fn sync_after_table_clear_only_covers_new_messages() {
        let mut state = CompactionState {
            synced_through_seq: 5,
            ..Default::default()
        };
        let messages: Vec<Message> = (0..7).map(|i| plan_message(&format!("m{i}"))).collect();
        let seqs: Vec<u64> = (0..7).collect();
        sync_blocks(&mut state, &messages, &seqs);
        assert_eq!(state.blocks.len(), 1);
        assert_eq!(state.blocks[0].spans, vec![SeqRange { start: 6, end: 6 }]);
        assert_eq!(state.synced_through_seq, 6);
    }

    struct ScriptedAdvisor {
        output: serde_json::Value,
    }

    impl wf_plugin_sdk::PluginCompactionAdvisor for ScriptedAdvisor {
        fn advise(
            &self,
            _input: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Ok(self.output.clone())
        }
    }

    #[test]
    fn advisor_suggestion_applies_to_every_strategy_including_builtin() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({"items": [{"block_id": "b0000", "start": 0, "end": 2, "depth": 0}]}),
            });
        let plan = try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false,
        )
        .expect("valid suggestion applies to every strategy including builtin");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].block_id, "b0000");
    }

    #[test]
    fn advisor_unknown_blocks_fall_back_to_builtin() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({"items": [{"block_id": "b9999", "start": 0, "end": 2}]}),
            });
        assert!(try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false
        )
        .is_none());
    }

    #[test]
    fn advisor_directive_trims_and_travels_with_plan() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({
                    "items": [{"block_id": "b0000", "start": 0, "end": 2, "depth": 0}],
                    "directive": "  focus on decisions  ",
                }),
            });
        let plan = try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false,
        )
        .expect("directive rides alongside the items");
        assert_eq!(plan.directive.as_deref(), Some("focus on decisions"));
        assert_eq!(plan.items.len(), 1);
    }

    #[test]
    fn advisor_blank_directive_means_no_directive() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({
                    "items": [{"block_id": "b0000", "start": 0, "end": 2, "depth": 0}],
                    "directive": "   ",
                }),
            });
        let plan = try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false,
        )
        .expect("blank directive keeps the plan");
        assert_eq!(plan.directive, None);
    }

    #[test]
    fn advisor_directive_only_defers_to_builtin() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({"directive": "focus on decisions"}),
            });
        assert!(try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false
        )
        .is_none());
    }

    #[test]
    fn advisor_overlong_directive_rejects_whole_suggestion() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let long = "x".repeat(DIRECTIVE_MAX_CHARS + 1);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({
                    "items": [{"block_id": "b0000", "start": 0, "end": 2, "depth": 0}],
                    "directive": long,
                }),
            });
        assert!(try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false
        )
        .is_none(),
        "over-long directive falls back to the built-in plan");
    }

    #[test]
    fn advisor_protocol_mismatch_falls_back_to_builtin() {
        let limits = limits();
        let messages: Vec<Message> = (0..4).map(|i| plan_message(&format!("m{i}"))).collect();
        let state = plan_state(vec![raw_block("b0000", 0, 1, 20_000)]);
        let advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor> =
            std::sync::Arc::new(ScriptedAdvisor {
                output: serde_json::json!({"protocol_version": 999, "items": [{"block_id": "b0000", "start": 0, "end": 2}]}),
            });
        assert!(try_advisor_plan_with(
            "builtin", &advisor, &state, &messages, 90_000, &limits, false
        )
        .is_none());
    }

    #[test]
    fn stateless_plan_factor_above_one() {
        assert!(stateless_plan_factor(&limits()) > 1.0);
    }

    #[test]
    fn plan_validators_enforce_snapshot_and_id_contract() {
        let good = CompactionPlan {
            items: vec![CompactionPlanItem {
                block_id: "b0000".to_string(),
                start: 0,
                end: 2,
                depth: 0,
                extra_block_ids: Vec::new(),
            }],
            drops: Vec::new(),
            freed_tokens: 0,
            capped: false,
            directive: None,
        };
        assert!(plan_covers_snapshot(&good, 2));
        assert!(!plan_covers_snapshot(&good, 1));
        let outcome = BlockOutcome {
            block_id: "b0000".to_string(),
            summary_message_id: "s".to_string(),
            archive_refs: Vec::new(),
        };
        assert!(outcomes_match_plan(
            std::slice::from_ref(&outcome),
            &good.items
        ));
        let foreign = BlockOutcome {
            block_id: "b9999".to_string(),
            summary_message_id: "s".to_string(),
            archive_refs: Vec::new(),
        };
        assert!(!outcomes_match_plan(
            std::slice::from_ref(&foreign),
            &good.items
        ));
        assert!(outcomes_match_plan(std::slice::from_ref(&foreign), &[]));
    }

    fn assert_full_coverage(items: &[CompactionPlanItem], len: usize) {
        let mut cursor = 0usize;
        let mut ids = std::collections::HashSet::new();
        for item in items {
            assert_eq!(item.start, cursor);
            assert!(ids.insert(item.block_id.clone()), "block ids stay unique");
            cursor = item.end;
        }
        assert_eq!(cursor, len);
    }

    #[test]
    fn stateless_plan_covers_every_index() {
        let costs = vec![50, 60, 70, 80, 90];
        let items = build_stateless_plan(&costs, 100, 8);
        assert_eq!(items.len(), 5);
        assert_full_coverage(&items, costs.len());
        assert!(items.iter().all(|item| item.block_id.starts_with('w')));
    }

    #[test]
    fn stateless_plan_widens_past_the_cap() {
        let costs = vec![10; 20];
        let items = build_stateless_plan(&costs, 15, 4);
        assert!(items.len() <= 4);
        assert_full_coverage(&items, costs.len());
    }

    #[test]
    fn adjacent_small_items_merge() {
        let messages = vec![
            plan_message("aaa"),
            plan_message("bbb"),
            plan_message("ccc"),
            plan_message("ddd"),
        ];
        let seqs = vec![0, 1, 2, 3];
        let mut budgeted = limits();
        budgeted.summary_call_budget_tokens = Some(u64::MAX);
        budgeted.plan_max_items = Some(8);
        let blocks = vec![
            raw_block("b0000", 0, 1, 20_000),
            raw_block("b0001", 2, 3, 20_000),
        ];
        let plan = build_plan(
            &plan_state(blocks),
            &MessageView::Full,
            &messages,
            &messages,
            &seqs,
            90_000,
            &budgeted,
            false,
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].block_id, "b0000");
        assert_eq!(plan.items[0].extra_block_ids, vec!["b0001".to_string()]);
        assert_eq!((plan.items[0].start, plan.items[0].end), (0, 4));
        assert_eq!(plan.freed_tokens, 40_000);
    }
}
