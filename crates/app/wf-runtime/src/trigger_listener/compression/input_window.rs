use wf_types::message::Message;

/// Headroom applied to the emission budget when trimming the summary input
/// snapshot (90%): the summary call must fit its own model window, which the
/// uncompressed snapshot by definition does not (self-reference guard).
const SUMMARY_INPUT_HEADROOM_NUM: u64 = 9;
const SUMMARY_INPUT_HEADROOM_DEN: u64 = 10;

/// Planned slice of the headroom-trimmed snapshot: one summary chain run
/// covers every block id listed, and a single aggregated write-back follows.
pub(super) struct PlannedSlice {
    pub(super) covered_ids: Vec<String>,
    pub(super) item_depth: u32,
    pub(super) messages: Vec<Message>,
}

pub(super) fn trim_messages_to_budget(
    mut messages: Vec<Message>,
    budget_tokens: u64,
    min_keep: usize,
) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }
    let min_keep = min_keep.max(1).min(messages.len());
    let costs: Vec<u64> = messages
        .iter()
        .map(|message| wf_llm::count::estimate_message_tokens(message) as u64)
        .collect();
    let mut total: u64 = costs.iter().fold(0, |acc, cost| acc.saturating_add(*cost));
    let mut drop_count = 0usize;
    while messages.len() - drop_count > min_keep && total > budget_tokens {
        total = total.saturating_sub(costs[drop_count]);
        drop_count += 1;
    }
    if drop_count > 0 {
        messages.drain(..drop_count);
    }
    messages
}

pub(super) fn build_degraded_notice(error: &str, dropped: usize) -> Message {
    let reason: String = error.chars().take(160).collect();
    Message::system_text(format!(
        "[context compression notice] Automatic history compression failed ({reason}): the \
         {dropped} oldest message(s) were dropped without a summary. The messages below are \
         the retained recent window; earlier context is unavailable."
    ))
}

/// Trim the emission snapshot to the summary input headroom, accounting the
/// request-level directive against the same budget. A zero token limit
/// skips trimming (no budget to fit).
pub(super) fn trim_snapshot(
    messages: Vec<Message>,
    token_limit: u64,
    directive: &str,
    tail_keep: usize,
) -> Vec<Message> {
    if token_limit == 0 {
        return messages;
    }
    let directive_tokens = if directive.is_empty() {
        0
    } else {
        u64::from(wf_llm::count::estimate_message_tokens(
            &Message::system_text(directive.to_string()),
        ))
    };
    trim_messages_to_budget(
        messages,
        (token_limit * SUMMARY_INPUT_HEADROOM_NUM / SUMMARY_INPUT_HEADROOM_DEN)
            .saturating_sub(directive_tokens),
        tail_keep,
    )
}

/// Slice the headroom-trimmed snapshot into one chain run per plan item,
/// heading every slice with the request-level directive. An empty plan is
/// the legacy single-summary mode (one item over the whole snapshot).
pub(super) fn slice_snapshot(
    snapshot: &[Message],
    plan: &[wf_execution_shared::CompactionPlanItem],
    directive: &str,
) -> Vec<PlannedSlice> {
    let head_directive = |slice: &[Message]| -> Vec<Message> {
        if directive.is_empty() {
            return slice.to_vec();
        }
        let mut headed = Vec::with_capacity(slice.len() + 1);
        headed.push(Message::system_text(directive.to_string()));
        headed.extend(slice.iter().cloned());
        headed
    };
    if plan.is_empty() {
        return vec![PlannedSlice {
            covered_ids: Vec::new(),
            item_depth: 0,
            messages: head_directive(snapshot),
        }];
    }
    plan.iter()
        .filter_map(|item| {
            let (start, end) = item.clamped_range(snapshot.len())?;
            let mut covered = vec![item.block_id.clone()];
            covered.extend(item.extra_block_ids.iter().cloned());
            Some(PlannedSlice {
                covered_ids: covered,
                item_depth: item.depth,
                messages: head_directive(&snapshot[start..end]),
            })
        })
        .collect()
}
