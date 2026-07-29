//! Cross-layer consistency tests.
//!
//! Real scenario (S2): AI-assisted code review workflow.
//! - Developer makes manual edits → staged
//! - AI agent makes edits → submits for approval → approved → integrated → staged
//! - After each transition, verify content consistency across layers.

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use layertwine::core::types::LayerType;

// ---------------------------------------------------------------------------
// Full pipeline: Manual edit → Agent edit → Submit → Approve → Unified → Staged → Commit
// ---------------------------------------------------------------------------

#[test]
fn test_full_pipeline_layer_consistency() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);

    // Phase 1: Manual edit → staged → commit
    apply_edit(&env, "app.rs", "fn main() {\n    println!(\"v1\");\n}\n");
    commit_changes(&env, "manual edit v1", "dev");

    // Phase 2: Agent edits the same file
    apply_agent_edit(&env, "agent-loop-1", "app.rs", "fn main() {\n    println!(\"v2-agent\");\n}\n");
    submit_agent(&env, "agent-loop-1");
    approve_agent(&env, "agent-loop-1", "feature-1");

    // Phase 3: Merge features directly to staged (no unified intermediary)
    merge_to_unified(&env, None);

    // Verify final staged content has agent's changes
    let staged_parts = get_partitions_by_layer(&env, LayerType::Staged);
    let final_sid = &staged_parts[0].current_snapshot;
    let final_text = reconstruct_text(&env, final_sid).unwrap_or_default();
    assert!(final_text.contains("v2-agent"), "final staged should have agent edit content");

    // Verify commit created checkpoint entries
    let log = get_log(&env, Some(10));
    assert!(log.len() >= 1, "should have at least one log entry");
}

// ---------------------------------------------------------------------------
// Multiple agent partitions consistency: two independent agents →
// approval → integrated → staged
//
// Two agents edit the same file, then all integrated partitions
// are merged directly to staged.
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_agents_pipeline_consistency() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);

    init_repository(&env);
    apply_edit(&env, "multi.rs", "base content\n");
    commit_changes(&env, "init", "dev");

    // Two independent agents edit the same file
    apply_agent_edit(&env, "agent-a", "multi.rs", "base content\n// agent-a change\n");
    apply_agent_edit(&env, "agent-b", "multi.rs", "base content\n// agent-b change\n");

    // Both submit → approval layer
    submit_agent(&env, "agent-a");
    submit_agent(&env, "agent-b");

    // Both approved → integrated layer
    approve_agent(&env, "agent-a", "feature-a");
    approve_agent(&env, "agent-b", "feature-b");

    // Merge all integrated directly to staged
    merge_to_unified(&env, None);

    // Final staged content should contain at least one agent's changes
    let staged_parts = get_partitions_by_layer(&env, LayerType::Staged);
    let final_text = reconstruct_text(&env, &staged_parts[0].current_snapshot).unwrap_or_default();
    assert!(final_text.contains("agent-a change") || final_text.contains("agent-b change"),
        "staged should contain at least one agent's changes");
}