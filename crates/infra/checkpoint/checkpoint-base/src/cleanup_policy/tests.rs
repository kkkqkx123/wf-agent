use super::*;

fn make_checkpoint(
    id: &str,
    timestamp_ms: i64,
    blob_size: Option<u64>,
) -> CheckpointStorageMetadata {
    CheckpointStorageMetadata {
        id: id.to_string(),
        entity_type: "test".to_string(),
        entity_id: "entity-1".to_string(),
        parent_entity_id: None,
        checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
        timestamp: timestamp_ms,
        status: wf_types::checkpoint::CheckpointStatus::Completed,
        previous_checkpoint_id: None,
        base_checkpoint_id: None,
        chain_root_id: None,
        chain_position: None,
        blob_size,
        tags: None,
        custom_fields: None,
    }
}

fn make_delta_checkpoint(
    id: &str,
    timestamp_ms: i64,
    previous: Option<&str>,
    chain_root: Option<&str>,
) -> CheckpointStorageMetadata {
    let mut cp = make_checkpoint(id, timestamp_ms, Some(100));
    cp.checkpoint_type = wf_types::checkpoint::CheckpointType::Delta;
    cp.previous_checkpoint_id = previous.map(String::from);
    cp.chain_root_id = chain_root.map(String::from);
    cp
}

#[test]
fn time_based_cleanup() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("old", now - 3_600_000, None),
        make_checkpoint("new", now - 1_000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::TimeBased {
            max_age_seconds: 1800,
            min_retention: 1,
        },
        &clock,
    );
    assert_eq!(to_remove, vec!["old"]);
}

#[test]
fn time_based_min_retention_keeps_newest() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("old-1", now - 7_200_000, None),
        make_checkpoint("old-2", now - 7_000_000, None),
        make_checkpoint("old-3", now - 6_800_000, None),
        make_checkpoint("new", now - 1_000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::TimeBased {
            max_age_seconds: 3600,
            min_retention: 2,
        },
        &clock,
    );
    assert_eq!(to_remove.len(), 2);
    assert!(!to_remove.contains(&"old-3".to_string()));
    assert!(!to_remove.contains(&"new".to_string()));
}

#[test]
fn count_based_cleanup() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("c1", now - 5000, None),
        make_checkpoint("c2", now - 4000, None),
        make_checkpoint("c3", now - 3000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 2,
            min_retention: 1,
        },
        &clock,
    );
    assert_eq!(to_remove, vec!["c1"]);
}

#[test]
fn count_based_under_limit() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![make_checkpoint("c1", now - 5000, None)];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 10,
            min_retention: 1,
        },
        &clock,
    );
    assert!(to_remove.is_empty());
}

#[test]
fn size_based_uses_real_blob_size() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("big", now - 5000, Some(8000)),
        make_checkpoint("small", now - 4000, Some(1000)),
        make_checkpoint("tiny", now - 3000, Some(500)),
    ];
    let executor = CleanupExecutor::new();
    // Total = 9500; removing "big" (8000) gets us under 2000.
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::SizeBased {
            max_total_bytes: 2000,
            min_retention: 1,
        },
        &clock,
    );
    assert_eq!(to_remove, vec!["big"]);
}

#[test]
fn size_based_result_reports_freed_bytes() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("c1", now - 5000, Some(1000)),
        make_checkpoint("c2", now - 4000, Some(2000)),
        make_checkpoint("c3", now - 3000, Some(500)),
    ];
    let executor = CleanupExecutor::new();
    let result = executor.evaluate_protected_with_result(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 1,
            min_retention: 0,
        },
        &clock,
    );
    assert_eq!(result.deleted_count, 2);
    assert_eq!(result.freed_bytes, 3000);
    assert_eq!(result.remaining_count, 1);
    assert_eq!(result.deleted_checkpoint_ids, vec!["c1", "c2"]);
}

#[test]
fn tiered_cleanup_keeps_one_per_window() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let day = DAY_MS;
    let checkpoints = vec![
        make_checkpoint("d10-1", now - 10 * day, None),
        make_checkpoint("d10-2", now - 10 * day - 1, None),
        make_checkpoint("d9", now - 9 * day, None),
        make_checkpoint("recent", now - 1_000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::Tiered {
            tiers: vec![RetentionTier::new(7, None, 1)],
            min_retention: 1,
        },
        &clock,
    );
    // Only the two checkpoints sharing a 1-day window are candidates;
    // the newest (recent) is protected by min_retention.
    assert_eq!(to_remove.len(), 1);
    assert!(!to_remove.contains(&"recent".to_string()));
}

#[test]
fn tiered_zero_interval_keeps_all() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let day = DAY_MS;
    let checkpoints = vec![
        make_checkpoint("a", now - 30 * day, None),
        make_checkpoint("b", now - 20 * day, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::Tiered {
            tiers: vec![RetentionTier::new(7, None, 0)],
            min_retention: 0,
        },
        &clock,
    );
    assert!(to_remove.is_empty());
}

#[test]
fn tiered_max_age_excludes_newer_checkpoints() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let day = DAY_MS;
    let checkpoints = vec![
        make_checkpoint("d30", now - 30 * day, None),
        make_checkpoint("d5", now - 5 * day, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::Tiered {
            tiers: vec![RetentionTier::new(7, Some(14), 0)],
            min_retention: 0,
        },
        &clock,
    );
    assert!(to_remove.is_empty(), "d5 is outside the tier age window");
}

#[test]
fn protection_keeps_chain_members_of_survivors() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("full-1", now - 4000, Some(100)),
        make_delta_checkpoint("delta-1", now - 3000, Some("full-1"), Some("full-1")),
        make_delta_checkpoint("delta-2", now - 2000, Some("delta-1"), Some("full-1")),
        make_delta_checkpoint("delta-3", now - 1000, Some("delta-2"), Some("full-1")),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate_protected(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 2,
            min_retention: 0,
        },
        &clock,
    );

    assert!(to_remove.is_empty(), "whole chain protected");
}

#[test]
fn protection_allows_removing_unreferenced_full() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("full-1", now - 4000, Some(100)),
        make_checkpoint("full-2", now - 3000, Some(100)),
        make_delta_checkpoint("delta-1", now - 2000, Some("full-2"), Some("full-2")),
        make_delta_checkpoint("delta-2", now - 1000, Some("delta-1"), Some("full-2")),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate_protected(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 2,
            min_retention: 0,
        },
        &clock,
    );

    assert_eq!(to_remove, vec!["full-1"]);
}

#[test]
fn chain_group_protection_keeps_deltas_of_surviving_baseline() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let day = 86_400_000i64;
    let checkpoints = vec![
        // Baseline and the other baseline survive (recent).
        make_checkpoint("full-1", now - 1_000, Some(100)),
        make_checkpoint("full-2", now - 2_000, Some(100)),
        // Old deltas are candidates, but their baseline full-1 survives.
        make_delta_checkpoint("delta-1", now - 3 * day, Some("full-1"), Some("full-1")),
        make_delta_checkpoint("delta-2", now - 2 * day, Some("delta-1"), Some("full-1")),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate_protected(
        &checkpoints,
        &CleanupStrategy::TimeBased {
            max_age_seconds: 86_400,
            min_retention: 0,
        },
        &clock,
    );

    assert!(
        to_remove.is_empty(),
        "deltas protected because their baseline full-1 survives"
    );
}

#[test]
fn tiered_clamps_future_timestamps_to_zero_age() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let day = DAY_MS;
    let checkpoints = vec![
        make_checkpoint("future-1", now + day, None),
        make_checkpoint("future-2", now + day + 1, None),
        make_checkpoint("recent", now - 1_000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::Tiered {
            tiers: vec![RetentionTier::new(0, None, 1)],
            min_retention: 1,
        },
        &clock,
    );
    assert!(
        !to_remove.contains(&"future-2".to_string()),
        "newest (future-2) wins the window tie-break"
    );
}

#[test]
fn time_based_retains_future_timestamps() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("future", now + 3_600_000, None),
        make_checkpoint("old", now - 7_200_000, None),
    ];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::TimeBased {
            max_age_seconds: 3600,
            min_retention: 0,
        },
        &clock,
    );
    assert_eq!(to_remove, vec!["old"]);
}

#[test]
fn count_cleanup_is_deterministic_on_timestamp_ties() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let checkpoints = vec![
        make_checkpoint("b", now, None),
        make_checkpoint("a", now, None),
        make_checkpoint("c", now, None),
    ];
    let executor = CleanupExecutor::new();
    let first = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 1,
            min_retention: 0,
        },
        &clock,
    );
    let second = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 1,
            min_retention: 0,
        },
        &clock,
    );
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
}

#[test]
fn time_based_fails_closed_without_clock() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let handle = clock.manual_handle().expect("manual clock has handle");
    handle.fail();
    let checkpoints = vec![make_checkpoint("old", now - 7_200_000, None)];
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate(
        &checkpoints,
        &CleanupStrategy::TimeBased {
            max_age_seconds: 3600,
            min_retention: 0,
        },
        &clock,
    );
    assert!(to_remove.is_empty());
}

#[test]
fn retention_bridge_maps_count_and_age() {
    let strategies = cleanup_strategies_from_retention(&CheckpointRetentionConfig {
        max_checkpoints: Some(1000),
        max_age: Some(604_800_000),
        compression: None,
    });
    assert_eq!(strategies.len(), 2);
    assert!(matches!(
        strategies[0],
        CleanupStrategy::CountBased {
            max_checkpoints: 1000,
            min_retention: 1
        }
    ));
    assert!(matches!(
        strategies[1],
        CleanupStrategy::TimeBased {
            max_age_seconds: 604_800,
            min_retention: 1
        }
    ));
}

#[test]
fn retention_bridge_rounds_age_up_to_seconds() {
    let strategies = cleanup_strategies_from_retention(&CheckpointRetentionConfig {
        max_checkpoints: None,
        max_age: Some(1500),
        compression: None,
    });
    assert_eq!(strategies.len(), 1);
    assert!(matches!(
        strategies[0],
        CleanupStrategy::TimeBased {
            max_age_seconds: 2,
            min_retention: 1
        }
    ));

    let sub_second = cleanup_strategies_from_retention(&CheckpointRetentionConfig {
        max_checkpoints: None,
        max_age: Some(1),
        compression: None,
    });
    assert!(matches!(
        sub_second[0],
        CleanupStrategy::TimeBased {
            max_age_seconds: 1,
            ..
        }
    ));
}

#[test]
fn retention_bridge_ignores_empty_and_non_positive_age() {
    let empty = cleanup_strategies_from_retention(&CheckpointRetentionConfig {
        max_checkpoints: None,
        max_age: None,
        compression: None,
    });
    assert!(empty.is_empty());

    let non_positive = cleanup_strategies_from_retention(&CheckpointRetentionConfig {
        max_checkpoints: None,
        max_age: Some(0),
        compression: None,
    });
    assert!(non_positive.is_empty());
}

#[test]
fn protected_cleanup_retires_linked_full_generations() {
    let now = 1_700_000_000_000i64;
    let clock = CheckpointClock::manual(now);
    let mut checkpoints = vec![
        make_checkpoint("full-1", now - 4000, Some(100)),
        make_checkpoint("full-2", now - 3000, Some(100)),
        make_checkpoint("full-3", now - 2000, Some(100)),
        make_checkpoint("full-4", now - 1000, Some(100)),
    ];
    for i in 1..checkpoints.len() {
        let prev = checkpoints[i - 1].id.clone();
        checkpoints[i].previous_checkpoint_id = Some(prev);
    }
    let executor = CleanupExecutor::new();
    let to_remove = executor.evaluate_protected(
        &checkpoints,
        &CleanupStrategy::CountBased {
            max_checkpoints: 2,
            min_retention: 0,
        },
        &clock,
    );
    assert_eq!(to_remove, vec!["full-1", "full-2"]);
}
