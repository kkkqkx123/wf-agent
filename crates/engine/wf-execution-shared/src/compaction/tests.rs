use super::*;
use wf_types::config::limits::CompactionLimits;
use wf_types::message::{
    BlockState, CompactionState, CompressionBlock, Message, MessageView, SeqRange,
};

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
        engine: None,
        engine_config: None,
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
