use std::collections::{HashMap, HashSet};

use wf_types::config::limits::CompactionLimits;
use wf_types::message::{
    BlockOutcome, BlockState, CompactionPlanItem, CompactionState, CompressionBlock, Message,
    MessageView,
};

use super::budget::{
    k_repress, r_min_range, r_repress, resolve_compaction_thresholds, resolve_summary_call_budget,
    stateless_plan_factor, PLAN_MAX_ITEMS_DEFAULT,
};
use super::strategy::{
    compaction_strategy_name, get_compaction_advisor, is_single_summary_strategy,
    ADVISOR_PROTOCOL_VERSION,
};

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

/// Workflow plan boundary: workflow targets keep no block table, so the
/// workflow path always uses the stateless plan. The wrapper forwards to
/// the shared estimator so a future layered workflow only replaces this
/// boundary. Request-level directives never apply here.
pub fn stateless_plan_for_workflow(
    messages: &[Message],
    estimated: u64,
    context_limit: u64,
    limits: &CompactionLimits,
) -> Vec<CompactionPlanItem> {
    stateless_plan_for_estimate(messages, estimated, context_limit, limits)
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
pub(crate) fn try_advisor_plan_with(
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
