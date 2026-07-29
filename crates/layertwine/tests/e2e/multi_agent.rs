//! Multi-agent collaboration E2E tests

use crate::common::assertions::*;
use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::ApiService;
use layertwine::core::types::{AgentInstanceId, SnapshotId};
use layertwine::storage::repository::PartitionStore;

#[test]
fn test_single_agent_workflow() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_single_agent_workflow");

    print_info("Step 1: Initialize repository with base content");
    init_repository(&env);
    let base_content = "Base content\n";
    apply_edit(&env, "shared.txt", base_content);
    commit_changes(&env, "Initial commit", "user-1");
    print_success("Base content committed");

    // Agent 1 edit
    print_info("Step 2: Agent-1 edits the file");
    let agent1_content = "Base content\nAgent-1 addition";
    let agent1_snapshot = apply_agent_edit(&env, "agent-1", "shared.txt", agent1_content);
    print_success(&format!(
        "Agent-1 edit applied, snapshot_id: {}",
        agent1_snapshot.to_hex()
    ));

    // Verify agent_edit layer
    print_info("Step 3: Verify agent_edit layer");
    let agent_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::AgentEdit);
    assert!(
        !agent_partitions.is_empty(),
        "agent_edit layer should have partitions"
    );

    for partition in &agent_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Submit agent
    print_info("Step 4: Agent-1 submits changes");
    let submit_snapshot = submit_agent(&env, "agent-1");
    print_success(&format!(
        "Agent-1 submitted, snapshot_id: {}",
        submit_snapshot.to_hex()
    ));

    // Verify approval layer
    print_info("Step 5: Verify approval layer");
    let approval_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Approval);
    assert!(
        !approval_partitions.is_empty(),
        "approval layer should have partitions"
    );

    for partition in &approval_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Approve agent
    print_info("Step 6: Approve Agent-1");
    let approved_snapshot = approve_agent(&env, "agent-1", "feature-1");
    print_success(&format!(
        "Agent-1 approved, snapshot_id: {}",
        approved_snapshot.to_hex()
    ));

    // Verify integrated layer
    print_info("Step 7: Verify integrated layer");

    // Debug: print all partitions
    let all_partitions = env.storage.list_partitions().unwrap_or_default();
    print_info(&format!("  Total partitions: {}", all_partitions.len()));
    for p in &all_partitions {
        print_info(&format!("    - {} (type: {:?})", p.name, p.partition_type));
    }

    let integrated_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Integrated);
    assert!(
        !integrated_partitions.is_empty(),
        "integrated layer should have partitions"
    );

    for partition in &integrated_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Merge features directly to staged (no unified intermediary)
    print_info("Step 7.5: Merge features directly to staged");
    let staged_snapshot_id = merge_to_unified(&env, None);
    print_success(&format!(
        "Merged features to staged, snapshot_id: {}",
        staged_snapshot_id.to_hex()
    ));

    // Verify staged layer
    print_info("Step 8: Verify staged layer");
    let staged_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Staged);
    assert!(
        !staged_partitions.is_empty(),
        "staged layer should have partitions"
    );

    // Commit to checkpoint
    print_info("Step 10: Commit staged changes");
    commit_changes(&env, "Merge Agent-1 feature", "user-1");
    print_success("Changes committed");

    // Verify final content
    print_info("Step 11: Verify final content");
    let reconstructed = reconstruct_text(&env, &approved_snapshot);
    assert!(reconstructed.is_some(), "Failed to reconstruct text");

    let actual_content = reconstructed.unwrap();
    print_file_content(&actual_content, 5);
    assert_eq!(actual_content, agent1_content, "Content mismatch");

    // Verify log
    print_info("Step 12: Verify commit history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 2);

    // Final state
    print_info("Final state verification");
    let all_partitions = env
        .storage
        .list_partitions()
        .expect("Failed to list partitions");
    print_all_layer_states(&all_partitions);

    print_test_result(true, "test_single_agent_workflow", None);
}

#[test]
fn test_two_agents_sequential() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_two_agents_sequential");

    print_info("Step 1: Initialize repository with base content");
    init_repository(&env);
    let base_content = "Base line\n";
    apply_edit(&env, "shared.txt", base_content);
    commit_changes(&env, "Initial commit", "user-1");
    print_success("Base content committed");

    // Agent 1 workflow
    print_info("Step 2: Agent-1 workflow");
    let agent1_content = "Base line\nAgent-1 addition";
    apply_agent_edit(&env, "agent-1", "shared.txt", agent1_content);
    submit_agent(&env, "agent-1");
    approve_agent(&env, "agent-1", "feature-1");
    print_success("Agent-1 workflow completed");

    // Verify integrated has feature-1
    print_info("Step 3: Verify feature-1 in integrated layer");

    // Debug: print all partitions
    let all_partitions = env.storage.list_partitions().unwrap_or_default();
    print_info(&format!("  Total partitions: {}", all_partitions.len()));
    for p in &all_partitions {
        print_info(&format!("    - {} (type: {:?})", p.name, p.partition_type));
    }

    let integrated_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Integrated);
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-1")),
        "feature-1 should be in integrated layer"
    );

    // Agent 2 workflow (builds on Agent-1's changes)
    print_info("Step 4: Agent-2 workflow (builds on Agent-1)");
    let agent2_content = "Base line\nAgent-1 addition\nAgent-2 addition";
    apply_agent_edit(&env, "agent-2", "shared.txt", agent2_content);
    submit_agent(&env, "agent-2");
    approve_agent(&env, "agent-2", "feature-2");
    print_success("Agent-2 workflow completed");

    // Verify both features in integrated
    print_info("Step 5: Verify both features in integrated layer");
    let integrated_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Integrated);
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-1")),
        "feature-1 should still be in integrated layer"
    );
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-2")),
        "feature-2 should be in integrated layer"
    );

    // Merge integrated to unified
    print_info("Step 5.5: Merge integrated to unified layer");
    merge_to_unified(&env, None);
    print_success("Merged integrated to unified");

    // Merge unified to staged
    print_info("Step 5.6: Merge unified to staged layer");
    merge_to_staged(&env);
    print_success("Merged unified to staged");

    // Commit merged changes
    print_info("Step 6: Commit merged changes");
    commit_changes(&env, "Merge Agent-1 and Agent-2 features", "user-1");
    print_success("Merged changes committed");

    // Verify final content
    print_info("Step 7: Verify final content");
    let status = get_status(&env);
    let staged_partitions = status
        .partitions
        .iter()
        .filter(|p| p.layer == "staged")
        .collect::<Vec<_>>();

    assert!(
        !staged_partitions.is_empty(),
        "staged layer should have partitions"
    );

    for partition in &staged_partitions {
        let snapshot_id =
            SnapshotId::from_hex(&partition.current_snapshot).expect("Invalid snapshot ID");
        if let Some(content) = reconstruct_text(&env, &snapshot_id) {
            print_info(&format!("Staged partition '{}':", partition.name));
            print_file_content(&content, 5);
        }
    }

    // Verify log
    print_info("Step 8: Verify commit history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 2);

    // Final state
    print_info("Final state verification");
    let all_partitions = env
        .storage
        .list_partitions()
        .expect("Failed to list partitions");
    print_all_layer_states(&all_partitions);

    print_test_result(true, "test_two_agents_sequential", None);
}

#[test]
fn test_three_agents_parallel() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_three_agents_parallel");

    print_info("Step 1: Initialize repository with base content");
    init_repository(&env);
    let base_content = "Base line\nLine 2\nLine 3";
    apply_edit(&env, "shared.txt", base_content);
    commit_changes(&env, "Initial commit", "user-1");
    print_success("Base content committed");

    // All three agents edit in parallel (different lines)
    print_info("Step 2: Three agents edit different lines");

    // Agent-1 edits line 2
    print_info("  Agent-1 edits line 2");
    let agent1_content = "Base line\nAgent-1 modified line 2\nLine 3";
    apply_agent_edit(&env, "agent-1", "shared.txt", agent1_content);

    // Agent-2 edits line 3
    print_info("  Agent-2 edits line 3");
    let agent2_content = "Base line\nLine 2\nAgent-2 modified line 3";
    apply_agent_edit(&env, "agent-2", "shared.txt", agent2_content);

    // Agent-3 adds new line
    print_info("  Agent-3 adds new line");
    let agent3_content = "Base line\nLine 2\nLine 3\nAgent-3 new line";
    apply_agent_edit(&env, "agent-3", "shared.txt", agent3_content);

    print_success("All three agents completed edits");

    // Verify agent_edit layer has 3 partitions
    print_info("Step 3: Verify agent_edit layer");
    let agent_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::AgentEdit);
    assert_eq!(
        agent_partitions.len(),
        3,
        "agent_edit layer should have 3 partitions"
    );

    for partition in &agent_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Submit all agents
    print_info("Step 4: Submit all agents");
    submit_agent(&env, "agent-1");
    submit_agent(&env, "agent-2");
    submit_agent(&env, "agent-3");
    print_success("All agents submitted");

    // Approve all agents
    print_info("Step 5: Approve all agents");
    approve_agent(&env, "agent-1", "feature-1");
    approve_agent(&env, "agent-2", "feature-2");
    approve_agent(&env, "agent-3", "feature-3");
    print_success("All agents approved");

    // Verify integrated layer has 3 features
    print_info("Step 6: Verify integrated layer");
    let integrated_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Integrated);
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-1")),
        "feature-1 should be in integrated"
    );
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-2")),
        "feature-2 should be in integrated"
    );
    assert!(
        integrated_partitions
            .iter()
            .any(|p| p.name.contains("feature-3")),
        "feature-3 should be in integrated"
    );

    for partition in &integrated_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Merge integrated to unified
    print_info("Step 6.5: Merge integrated to unified layer");
    merge_to_unified(&env, None);
    print_success("Merged integrated to unified");

    // Merge unified to staged
    print_info("Step 6.6: Merge unified to staged layer");
    merge_to_staged(&env);
    print_success("Merged unified to staged");

    // Commit merged changes
    print_info("Step 7: Commit merged changes");
    commit_changes(&env, "Merge all three agent features", "user-1");
    print_success("Merged changes committed");

    // Verify log
    print_info("Step 8: Verify commit history");
    let log = get_log(&env, None);
    print_checkpoint_log(&log);
    assert_log_entry_count(&env, 2);

    // Final state
    print_info("Final state verification");
    let all_partitions = env
        .storage
        .list_partitions()
        .expect("Failed to list partitions");
    print_all_layer_states(&all_partitions);

    print_test_result(true, "test_three_agents_parallel", None);
}

#[test]
fn test_agent_rejection() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_agent_rejection");

    print_info("Step 1: Initialize repository with base content");
    init_repository(&env);
    let base_content = "Base content";
    apply_edit(&env, "shared.txt", base_content);
    commit_changes(&env, "Initial commit", "user-1");
    print_success("Base content committed");

    // Agent edit
    print_info("Step 2: Agent makes edit");
    let agent_content = "Base content\nAgent addition";
    apply_agent_edit(&env, "agent-1", "shared.txt", agent_content);
    print_success("Agent edit applied");

    // Submit agent
    print_info("Step 3: Submit agent");
    submit_agent(&env, "agent-1");
    print_success("Agent submitted");

    // Verify approval layer
    print_info("Step 4: Verify approval layer");
    let approval_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Approval);
    assert!(
        !approval_partitions.is_empty(),
        "approval layer should have partitions"
    );

    for partition in &approval_partitions {
        print_info(&format!(
            "  - Partition: {}, snapshot: {}",
            partition.name,
            truncate_id(&partition.current_snapshot.to_hex())
        ));
    }

    // Reject agent
    print_info("Step 5: Reject agent");
    let reject_response = env
        .api
        .reject_agent(layertwine::api::RejectAgentRequest {
            agent_id: "agent-1".to_string(),
        })
        .expect("Failed to reject agent");
    print_success(&format!(
        "Agent rejected, baseline_snapshot_id: {}",
        &reject_response.baseline_snapshot_id[..12]
    ));

    // Verify agent was removed from approval (should not be pending anymore)
    print_info("Step 6: Verify agent removed from approval");
    let approval_partitions =
        get_partitions_by_layer(&env, layertwine::core::types::LayerType::Approval);

    // Debug: print all approval partitions
    for p in &approval_partitions {
        if let layertwine::core::types::PartitionType::Approval(agent_id) = &p.partition_type {
            print_info(&format!(
                "  - Partition: {}, history.len: {}, current: {}",
                agent_id,
                p.history.len(),
                truncate_id(&p.current_snapshot.to_hex())
            ));
        }
    }

    // After rejection, the partition should still exist but not be pending (history.len() == 1)
    let _has_agent1 = approval_partitions.iter().any(|p| {
        if let layertwine::core::types::PartitionType::Approval(agent_id) = &p.partition_type {
            agent_id == &AgentInstanceId("agent-1".to_string())
        } else {
            false
        }
    });
    let agent1_pending = approval_partitions.iter().any(|p| {
        if let layertwine::core::types::PartitionType::Approval(agent_id) = &p.partition_type {
            agent_id == &AgentInstanceId("agent-1".to_string()) && p.history.len() > 1
        } else {
            false
        }
    });

    // The partition may still exist but should not be pending
    assert!(
        !agent1_pending,
        "Agent-1 should not be pending after rejection"
    );
    print_success("Agent-1 removed from approval");

    // Verify baseline was restored
    print_info("Step 7: Verify baseline restoration");
    let baseline_id =
        SnapshotId::from_hex(&reject_response.baseline_snapshot_id).expect("Invalid snapshot ID");
    let baseline_content = reconstruct_text(&env, &baseline_id);
    assert!(baseline_content.is_some(), "Failed to reconstruct baseline");

    let actual_baseline = baseline_content.unwrap();
    print_file_content(&actual_baseline, 5);
    assert_eq!(actual_baseline, base_content, "Baseline content mismatch");
    print_success("Baseline correctly restored");

    // Final state
    print_info("Final state verification");
    let all_partitions = env
        .storage
        .list_partitions()
        .expect("Failed to list partitions");
    print_all_layer_states(&all_partitions);

    print_test_result(true, "test_agent_rejection", None);
}

fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}
