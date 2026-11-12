use super::*;
use crate::token_tracker::RequestUsage;
use wf_llm::message_builder::{system_text, tool_result_message, user_text};
use wf_types::llm::MessageStreamUsage;
use wf_types::llm::TokenUsageStats;
use wf_types::message::{BlockOutcome, BlockState, CompactionMode, IndexEntry};
use wf_types::message::{CompressionBlock, SeqRange};

fn user(text: &str) -> Message {
    user_text(text)
}

#[test]
fn add_message_tracks_ledger_incrementally() {
    let mut session = ConversationSession::new();
    assert_eq!(session.estimated_conversation_tokens(), 0);
    assert_eq!(session.conversation_version(), 0);

    session.add_message(user("hello"));
    let v1 = session.conversation_version();
    let t1 = session.estimated_conversation_tokens();
    assert_eq!(v1, 1);
    assert!(t1 > 0);
    assert_eq!(session.state.messages.len(), 1);

    session.add_message(user("world"));
    let t2 = session.estimated_conversation_tokens();
    assert_eq!(session.conversation_version(), 2);
    assert!(t2 > t1, "estimate must grow with appends");
    assert_eq!(
        session.state.ledger.message_count(CONVERSATION_CONTEXT_ID),
        2
    );
}

#[test]
fn restore_history_marks_dirty_and_recomputes_lazily() {
    let mut session = ConversationSession::new();
    session.add_message(user("alpha"));
    session.add_message(user("beta"));
    let stale = session.estimated_conversation_tokens();

    let fresh = vec![user(
        "a much longer replacement message that costs more tokens",
    )];
    session.restore_history(fresh, MessageView::Full);
    assert!(session.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID));
    assert_eq!(
        session
            .state
            .ledger
            .estimated_tokens(CONVERSATION_CONTEXT_ID),
        0
    );

    let recomputed = session.estimated_conversation_tokens();
    assert!(!session.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID));
    assert_ne!(recomputed, stale);
    assert_eq!(
        session.state.ledger.message_count(CONVERSATION_CONTEXT_ID),
        1
    );
    // A second read must not recompute again (estimate is stable).
    assert_eq!(session.estimated_conversation_tokens(), recomputed);
}

#[test]
fn compress_switches_view_while_history_stays_append_only() {
    let mut session = ConversationSession::new();
    session.add_message(user("alpha"));
    session.add_message(user("beta"));
    let version_before = session.conversation_version();

    session.compress(vec![user("summary")]);

    // History grew: nothing was deleted.
    assert_eq!(session.history().len(), 3);
    assert_eq!(session.messages().len(), 3);
    assert!(session.conversation_version() > version_before);
    // The LLM projection shrank to the summary.
    let view = session.view_messages();
    assert_eq!(view.len(), 1);
    assert_ne!(session.view_messages().len(), session.history().len());
    assert!(matches!(
        session.active_view(),
        MessageView::Compacted { .. }
    ));
}

#[test]
fn undo_compression_restores_full_visibility_at_zero_cost() {
    let mut session = ConversationSession::new();
    session.add_message(user("alpha"));
    session.compress(vec![user("summary")]);
    assert_eq!(session.view_messages().len(), 1);

    session.restore_full_view();

    assert!(session.active_view().is_full());
    assert_eq!(session.view_messages().len(), session.history().len());
    assert_eq!(session.history().len(), 2);
}

#[test]
fn view_estimate_shrinks_after_compression() {
    let mut session = ConversationSession::new();
    session.add_message(user("a fairly long first message for tokens"));
    session.add_message(user("a fairly long second message for tokens"));
    let before = session.estimated_view_tokens();
    session.compress(vec![user("short")]);
    assert!(session.estimated_view_tokens() < before);
}

#[test]
fn compression_emission_guard_is_single_shot_per_version() {
    let mut session = ConversationSession::new();
    session.add_message(user("hello"));
    let v = session.conversation_version();

    assert!(session.should_emit_compression(v));
    session.mark_compression_emitted(v);
    assert!(!session.should_emit_compression(v));

    // A new append re-arms the guard for the new version.
    session.add_message(user("again"));
    let v2 = session.conversation_version();
    assert_ne!(v2, v);
    assert!(session.should_emit_compression(v2));
}

#[test]
fn snapshot_restore_roundtrip() {
    let mut session = ConversationSession::with_token_limit(100);
    session.add_message(user("hello"));
    session.update_estimated_usage(50, 30);
    session.finalize_current_request();
    assert!(session.consume_token_warning(50.0), "80% > 50% fires");

    let snapshot = session.snapshot_state();
    assert!(snapshot.tracker.is_some());
    assert_eq!(snapshot.messages.len(), 1);

    let mut restored = ConversationSession::new();
    restored.restore_state(snapshot);
    assert_eq!(restored.messages().len(), 1);
    assert!(restored.active_view().is_full());
    assert_eq!(restored.view_messages().len(), 1);
    assert_eq!(restored.estimated_total(), session.estimated_total());
    assert_eq!(
        restored.conversation_version(),
        session.conversation_version()
    );
    // The consumed warning must not fire again after restore.
    assert!(!restored.consume_token_warning(50.0));
    assert_eq!(restored.token_usage(), session.token_usage());
}

#[test]
fn token_limit_flow_drives_decisions() {
    let mut session = ConversationSession::with_token_limit(100);
    session.update_estimated_usage(60, 30);
    session.finalize_current_request(); // 90 cumulative
    assert!(!session.is_token_limit_exceeded());
    assert_eq!(session.estimated_total(), 90);
    assert!((session.usage_percentage().unwrap() - 90.0).abs() < f64::EPSILON);

    assert!(session.consume_token_warning(50.0), "90% > 50% fires");
    assert!(!session.consume_token_warning(50.0), "single-shot warning");

    assert!(
        session.consume_limit_exceeded_tier().is_none(),
        "90 <= 100: tier must not fire"
    );
    session.update_estimated_usage(80, 40);
    session.finalize_current_request(); // 210 total = 210% of 100
    assert!(session.is_token_limit_exceeded());
    assert_eq!(
        session.consume_limit_exceeded_tier(),
        Some(4),
        "210% crosses the 200-250% band (tier 4)"
    );
    assert!(
        session.consume_preflight_warning(),
        "preflight warning fires once"
    );
    assert!(!session.consume_preflight_warning());
}

#[test]
fn token_limit_zero_disables_checks() {
    let mut session = ConversationSession::new();
    session.update_estimated_usage(10_000, 10_000);
    assert!(!session.is_token_limit_exceeded());
    assert!(session.usage_percentage().is_none());
    assert!(!session.consume_token_warning(50.0));
    assert!(session.consume_limit_exceeded_tier().is_none());
}

#[test]
fn cost_track_merges_stream_usage_and_finalizes() {
    let mut session = ConversationSession::new();
    let usage = RequestUsage {
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
        reasoning_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        total_cost: Some(0.01),
        model: Some("gpt-4o".to_string()),
    };
    session.accumulate_stream_usage(&usage);
    assert_eq!(session.current_request_usage().total_tokens, 15);

    session.finalize_current_request();
    let stats = session.token_usage_stats().expect("usage recorded");
    assert_eq!(stats.total_tokens, 15);
    assert_eq!(session.token_usage(), 15);
    assert_eq!(stats.total_cost, Some(0.01));

    // Estimate track stays separate from the cost track.
    assert_eq!(session.estimated_total(), 0);
}

#[test]
fn cost_and_decision_tracks_stay_separate() {
    let mut session = ConversationSession::new();
    let stats = TokenUsageStats {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
        reasoning_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        prompt_tokens_cost: None,
        completion_tokens_cost: None,
        total_cost: Some(0.005),
        provider_total: None,
    };
    session.update_token_usage(&stats);
    // Real usage is present, but the decision track still queues the
    // estimated size of the request.
    session.accumulate_estimated_usage(90, 45);
    session.finalize_current_request();

    assert_eq!(
        session.token_usage_stats().unwrap().total_tokens,
        150,
        "cost track keeps the real provider usage"
    );
    assert_eq!(
        session.estimated_total(),
        135,
        "decision track keeps the estimate"
    );
    assert_eq!(
        session.billed_total(),
        150,
        "task-budget track prefers the provider usage"
    );
}

fn raw_block(id: &str, start: u64, end: u64) -> CompressionBlock {
    CompressionBlock {
        block_id: id.to_string(),
        state: BlockState::Raw,
        topic: String::new(),
        summary_message_id: String::new(),
        spans: vec![SeqRange { start, end }],
        merged_blocks: Vec::new(),
        size_tokens: 3000,
        source_tokens: 3000,
        repress_count: 0,
        active: true,
        created_at: 0,
        compressed_at: None,
    }
}

#[test]
fn single_summary_strategy_skips_block_sync() {
    let mut single = crate::compaction::builtin_compaction_limits();
    single.strategy = Some(crate::compaction::SINGLE_SUMMARY_STRATEGY_NAME.to_string());
    let mut session = ConversationSession::new();
    session.set_compaction_window(100_000, &single);
    assert!(!session.block_tracking_enabled());
    for i in 0..4 {
        session.add_message(user(&format!("message {i}")));
    }
    assert!(session.compaction_blocks().is_empty());
    // Threshold gating still works: the hot estimate is maintained.
    assert!(session.estimated_view_tokens() > 0);

    let mut layered = ConversationSession::new();
    layered.set_compaction_window(100_000, &crate::compaction::builtin_compaction_limits());
    assert!(layered.block_tracking_enabled());
    for i in 0..4 {
        layered.add_message(user(&format!("message {i}")));
    }
    assert_eq!(layered.compaction_blocks().len(), 1);
}

#[test]
fn applied_drops_leave_the_block_table() {
    let mut session = ConversationSession::new();
    for i in 0..6 {
        session.add_message(user(&format!("message {i}")));
    }
    session.state.compaction.blocks = vec![raw_block("b0000", 0, 2), raw_block("b0001", 3, 5)];
    session.state.compaction.synced_through_seq = 5;
    session.set_pending_compaction(Vec::new(), vec!["b0001".to_string()]);
    let summary = Message::system_text("summary of b0000".to_string());
    let outcome = BlockOutcome {
        block_id: "b0000".to_string(),
        summary_message_id: summary.id.clone(),
        archive_refs: Vec::new(),
    };
    session.apply_compaction_result(
        vec![summary],
        std::slice::from_ref(&outcome),
        &crate::compaction::builtin_compaction_limits(),
    );
    let blocks = &session.state.compaction.blocks;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].block_id, "b0000");
    assert_eq!(blocks[0].state, BlockState::Compressed);
}

#[test]
fn handoff_clears_inactive_blocks_but_keeps_index() {
    let mut session = ConversationSession::new();
    for i in 0..4 {
        session.add_message(user(&format!("message {i}")));
    }
    session.state.compaction.blocks = vec![raw_block("b0000", 0, 3)];
    session.state.compaction.synced_through_seq = 3;
    let document = Message::system_text("handoff document".to_string());
    let index = vec![IndexEntry {
        block_id: "b0000".to_string(),
        topic: "t".to_string(),
        time_range: "0..=3".to_string(),
        archive_ref: "a:blob".to_string(),
        keywords: Vec::new(),
        size_tokens: 3000,
    }];
    session.enter_handoff(document, index, "TEST", None, 2);
    assert!(session.state.compaction.blocks.is_empty());
    let handoff = session
        .state
        .compaction
        .handoff
        .as_ref()
        .expect("handoff artifact stays");
    assert_eq!(handoff.index.len(), 1);
    assert_eq!(handoff.index[0].archive_ref, "a:blob");
    assert_eq!(session.compaction_mode(), CompactionMode::Handoff);
    // Later appends sync as fresh raw blocks without resurrecting
    // the archived intervals.
    session.add_message(user("after handoff"));
    assert_eq!(session.state.compaction.blocks.len(), 1);
    assert_eq!(
        session.state.compaction.blocks[0].spans,
        vec![SeqRange { start: 5, end: 5 }]
    );
}

#[test]
fn incremental_view_estimate_matches_full_projection() {
    let mut session = ConversationSession::new();
    session.add_message(user("hello world, this is a test"));
    session.add_message(user("second message with more content here"));
    assert_eq!(
        session.estimated_view_tokens(),
        wf_llm::count::estimate_messages(&session.view_messages()) as u64
    );
    session.compress_with_tail(vec![user("summary of both")], 1);
    assert_eq!(
        session.estimated_view_tokens(),
        wf_llm::count::estimate_messages(&session.view_messages()) as u64
    );
    session.add_message(user("a follow-up after compression"));
    assert_eq!(
        session.estimated_view_tokens(),
        wf_llm::count::estimate_messages(&session.view_messages()) as u64
    );
}

#[test]
fn estimated_usage_fills_cost_track_when_provider_reports_none() {
    let mut session = ConversationSession::new();
    session.update_estimated_usage(90, 45);
    session.finalize_current_request();
    assert_eq!(session.token_usage_stats().unwrap().total_tokens, 135);
    assert_eq!(session.estimated_total(), 135);
    assert_eq!(session.token_usage(), 135);
}

#[test]
fn reset_clears_messages_and_tracking() {
    let mut session = ConversationSession::with_token_limit(100);
    session.add_message(user("hello"));
    session.update_estimated_usage(50, 50);
    session.finalize_current_request();
    session.reset();

    assert!(session.messages().is_empty());
    assert_eq!(session.estimated_total(), 0);
    assert_eq!(session.token_usage(), 0);
    assert_eq!(session.conversation_version(), 0);
    assert_eq!(session.token_limit(), 100, "limit survives reset");
}

#[test]
fn message_stream_usage_converts_to_request_usage() {
    let usage: RequestUsage = MessageStreamUsage {
        usage: TokenUsageStats {
            prompt_tokens: 7,
            completion_tokens: 3,
            total_tokens: 10,
            reasoning_tokens: Some(1),
            cache_read_tokens: Some(4),
            cache_write_tokens: Some(5),
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: Some(0.002),
            provider_total: None,
        },
    }
    .into();
    assert_eq!(usage.prompt_tokens, 7);
    assert_eq!(usage.total_tokens, 10);
    assert_eq!(usage.reasoning_tokens, Some(1));
    assert_eq!(usage.cache_read_tokens, Some(4));
    assert_eq!(usage.cache_write_tokens, Some(5));
    assert_eq!(usage.total_cost, Some(0.002));
}

#[test]
fn serde_roundtrip_of_conversation_state() {
    let mut session = ConversationSession::with_token_limit(1000);
    session.add_message(user("hello"));
    session.add_message(system_text("be helpful"));
    session.add_message(tool_result_message("call_1", "result"));
    session.update_estimated_usage(10, 20);
    session.finalize_current_request();

    let json = serde_json::to_string(&session.snapshot_state()).unwrap();
    let decoded: ConversationState = serde_json::from_str(&json).unwrap();
    let mut restored = ConversationSession::new();
    restored.restore_state(decoded);
    assert_eq!(restored.messages().len(), 3);
    assert_eq!(restored.estimated_total(), 30);
    assert_eq!(restored.token_usage(), 30);
}

#[test]
fn seq_coordinates_stay_stable_and_monotonic() {
    let mut session = ConversationSession::new();
    session.add_message(user("a"));
    session.add_message(user("b"));
    session.add_message(user("c"));
    assert_eq!(session.history().len(), session.message_seqs().len());
    assert_eq!(session.message_seqs(), &[0, 1, 2]);
    assert_eq!(session.seq_range(), Some((0, 2)));
    session.compress_with_tail(vec![user("summary")], 1);
    assert_eq!(session.history().len(), session.message_seqs().len());
    assert_eq!(session.message_seqs(), &[0, 1, 2, 3]);
    assert_eq!(session.next_seq(), 4);
}

#[test]
fn branch_prefix_leaves_source_untouched() {
    let mut session = ConversationSession::new();
    session.add_message(user("a"));
    session.add_message(user("b"));
    session.add_message(user("c"));
    let branch = session
        .branch_state_through_seq(1, MessageView::Full)
        .expect("seq present");
    assert_eq!(branch.messages.len(), 2);
    assert_eq!(branch.seqs, vec![0, 1]);
    assert_eq!(session.history().len(), 3);
    assert_eq!(session.message_seqs(), &[0, 1, 2]);
    let prefix = session.prefix_through_seq(1).expect("seq present");
    assert_eq!(prefix.len(), 2);
}

#[test]
fn range_view_resolves_against_session_coordinates() {
    let mut session = ConversationSession::new();
    session.add_message(user("a"));
    session.add_message(user("b"));
    session.add_message(user("c"));
    session.state.active_view = MessageView::Range {
        start_seq: 1,
        end_seq: 2,
    };
    let view = session.view_messages();
    assert_eq!(view.len(), 2);
    assert_eq!(view[0], session.history()[1]);
}
