use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

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

/// Metadata key carrying an archive pointer on a distilled message.
pub const ARCHIVE_REF_KEY: &str = "archive_ref";
/// Archive pointer prefix: `a:<blob-id>`.
pub const ARCHIVE_REF_PREFIX: &str = "a:";

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
    }
}

static PROCESS_LIMITS: OnceLock<CompactionLimits> = OnceLock::new();

/// Inject the process-wide compaction parameters once at bootstrap, mirroring
/// the compression settle-budget injection.
pub fn set_process_compaction_limits(limits: CompactionLimits) {
    let _ = PROCESS_LIMITS.set(limits);
}

/// Effective process-wide compaction parameters.
pub fn process_compaction_limits() -> CompactionLimits {
    PROCESS_LIMITS
        .get()
        .cloned()
        .unwrap_or_else(builtin_compaction_limits)
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
/// wins, otherwise one tenth of the model window clamped into a sane
/// band; a zero window means compaction is disabled and the default
/// applies so planning never divides by zero.
pub fn resolve_summary_call_budget(window_tokens: u64, limits: &CompactionLimits) -> u64 {
    if let Some(explicit) = limits.summary_call_budget_tokens {
        return explicit.max(1);
    }
    if window_tokens == 0 {
        return SUMMARY_CALL_BUDGET_DEFAULT;
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

/// Maximum summary-chain calls in one compaction round.
fn plan_max_items(limits: &CompactionLimits) -> usize {
    limits
        .plan_max_items
        .unwrap_or(PLAN_MAX_ITEMS_DEFAULT)
        .max(1)
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
}

impl CompactionPlan {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            drops: Vec::new(),
            freed_tokens: 0,
            capped: false,
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

/// Tool output class driving pre-history distillation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutputKind {
    Test,
    Search,
    Diff,
    File,
    Command,
}

/// Classify a tool result by tool name for distillation.
pub fn classify_tool(tool_name: &str) -> ToolOutputKind {
    let name = tool_name.to_lowercase();
    if name == "read_file" || name == "read-file" {
        return ToolOutputKind::File;
    }
    if name.contains("test") || name.contains("build") {
        return ToolOutputKind::Test;
    }
    if name.contains("search") || name.contains("grep") || name.contains("code_") {
        return ToolOutputKind::Search;
    }
    if name.contains("diff") {
        return ToolOutputKind::Diff;
    }
    ToolOutputKind::Command
}

fn take_head_tail(text: &str, head: usize, tail: usize) -> String {
    if text.len() <= head.saturating_add(tail).saturating_add(128) {
        return text.to_string();
    }
    let head_part: String = text.chars().take(head).collect();
    let tail_part: String = text
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!(
        "{head_part}\n…[truncated {0} chars]…\n{tail_part}",
        text.len()
    )
}

fn important_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error")
                || lower.contains("fail")
                || lower.contains("panic")
                || lower.contains("assert")
                || lower.contains("traceback")
                || line.contains(".rs:")
                || line.contains(".ts:")
                || line.contains(".py:")
        })
        .collect()
}

/// Distill one tool result before it enters history: keep conclusions,
/// errors and file references, drop bulk output. Returns the replacement
/// text; the original is archived by the caller.
pub fn distill_tool_output(kind: ToolOutputKind, text: &str) -> String {
    if text.len() < 2000 {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    match kind {
        ToolOutputKind::File => text.to_string(),
        ToolOutputKind::Test => {
            let hits = important_lines(&lines);
            let kept: Vec<&str> = hits.into_iter().take(60).collect();
            let tail: Vec<&str> = lines
                .iter()
                .rev()
                .take(25)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!(
                "[distilled test output: {0} lines -> {1} kept]\n{2}\n--- tail ---\n{3}",
                lines.len(),
                kept.len(),
                kept.join("\n"),
                tail.join("\n")
            )
        }
        ToolOutputKind::Search => {
            let kept: Vec<&str> = lines.iter().take(40).copied().collect();
            format!(
                "[distilled search output: {0} hits]\n{1}",
                lines.len(),
                kept.join("\n")
            )
        }
        ToolOutputKind::Diff => {
            let files: Vec<&&str> = lines
                .iter()
                .filter(|l| l.starts_with("diff --git") || l.starts_with("+++ b/"))
                .take(64)
                .collect();
            let added = lines.iter().filter(|l| l.starts_with('+')).count();
            let removed = lines.iter().filter(|l| l.starts_with('-')).count();
            format!(
                "[distilled diff: {0} files, +{1}/-{2}]\n{3}\n{4}",
                files.len(),
                added,
                removed,
                files
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                take_head_tail(text, 1500, 1000)
            )
        }
        ToolOutputKind::Command => take_head_tail(text, 3000, 1500),
    }
}

/// Content hash for file-read deduplication.
pub fn blake3_hex(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// Pointer text replacing a repeated file read.
pub fn file_pointer_text(path: &str, commit: &str, seq: u64) -> String {
    if commit.is_empty() {
        format!("[file {path} already read, see m{seq}]")
    } else {
        format!("[file {path} @ {commit} already read, see m{seq}]")
    }
}

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
    fn short_tool_output_passes_through() {
        assert_eq!(
            distill_tool_output(ToolOutputKind::Command, "ok"),
            "ok".to_string()
        );
    }

    #[test]
    fn long_command_output_truncates() {
        let text = "x\n".repeat(5000);
        let out = distill_tool_output(ToolOutputKind::Command, &text);
        assert!(out.len() < text.len());
        assert!(out.contains("truncated"));
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
            SUMMARY_CALL_BUDGET_DEFAULT
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

    #[test]
    fn stateless_plan_factor_above_one() {
        assert!(stateless_plan_factor(&limits()) > 1.0);
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
