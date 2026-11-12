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
/// skips trimming (no budget to fit). Returns the trimmed snapshot plus
/// the count of head messages dropped so plan indices built on the
/// untrimmed snapshot can shift forward before slicing.
pub(super) fn trim_snapshot(
    messages: Vec<Message>,
    token_limit: u64,
    directive: &str,
    tail_keep: usize,
) -> (Vec<Message>, usize) {
    if token_limit == 0 {
        return (messages, 0);
    }
    let directive_tokens = if directive.is_empty() {
        0
    } else {
        u64::from(wf_llm::count::estimate_message_tokens(
            &Message::system_text(directive.to_string()),
        ))
    };
    let original_len = messages.len();
    let trimmed = trim_messages_to_budget(
        messages,
        (token_limit * SUMMARY_INPUT_HEADROOM_NUM / SUMMARY_INPUT_HEADROOM_DEN)
            .saturating_sub(directive_tokens),
        tail_keep,
    );
    let dropped = original_len.saturating_sub(trimmed.len());
    (trimmed, dropped)
}

/// Shift plan indices forward by the head messages dropped during trimming.
/// Preserves block identity and depth; intervals collapsing to empty are
/// filtered so slicing never covers stale content. An empty return means
/// the whole planned prefix was trimmed away and the caller falls back to
/// the single-summary mode.
pub(super) fn shift_plan_for_trim(
    plan: &[wf_execution_shared::CompactionPlanItem],
    dropped: usize,
) -> Vec<wf_execution_shared::CompactionPlanItem> {
    if dropped == 0 {
        return plan.to_vec();
    }
    plan.iter()
        .filter_map(|item| {
            let start = item.start.saturating_sub(dropped);
            let end = item.end.saturating_sub(dropped);
            if start < end {
                Some(wf_execution_shared::CompactionPlanItem {
                    block_id: item.block_id.clone(),
                    start,
                    end,
                    depth: item.depth,
                    extra_block_ids: item.extra_block_ids.clone(),
                })
            } else {
                None
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn text_message(text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    fn plan_item(block: &str, start: usize, end: usize) -> wf_execution_shared::CompactionPlanItem {
        wf_execution_shared::CompactionPlanItem {
            block_id: block.to_string(),
            start,
            end,
            depth: 0,
            extra_block_ids: Vec::new(),
        }
    }

    #[test]
    fn shift_without_trim_keeps_plan() {
        let plan = vec![plan_item("b0000", 0, 2), plan_item("b0001", 2, 4)];
        assert_eq!(super::shift_plan_for_trim(&plan, 0), plan);
    }

    #[test]
    fn shift_past_trimmed_head_keeps_content_aligned() {
        let messages: Vec<Message> = (0..4).map(|i| text_message(&format!("m{i}"))).collect();
        let plan = vec![plan_item("b0000", 2, 4)];
        let shifted = super::shift_plan_for_trim(&plan, 2);
        assert_eq!(shifted.len(), 1);
        assert_eq!((shifted[0].start, shifted[0].end), (0, 2));
        let snapshot = &messages[2..];
        let items = super::slice_snapshot(snapshot, &shifted, "");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].messages.len(), 2);
    }

    #[test]
    fn shift_fully_trimmed_plan_falls_back_to_single_summary() {
        let messages: Vec<Message> = (0..2).map(|i| text_message(&format!("m{i}"))).collect();
        let plan = vec![plan_item("b0000", 0, 2)];
        let shifted = super::shift_plan_for_trim(&plan, 2);
        assert!(shifted.is_empty());
        let items = super::slice_snapshot(&messages, &shifted, "");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].messages.len(), 2);
    }
}
