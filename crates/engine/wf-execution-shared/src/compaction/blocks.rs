use std::collections::HashSet;

use wf_types::message::{
    BlockState, CompactionMode, CompactionState, CompressionBlock, Message, SeqRange,
};

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
