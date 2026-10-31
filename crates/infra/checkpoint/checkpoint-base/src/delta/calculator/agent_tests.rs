#[cfg(test)]
mod tests {
    use crate::delta::calculator::agent::*;
    use crate::delta::calculator::test_support::{make_agent_snapshot, make_message};
    use crate::delta::DiffCalculator;

    #[tokio::test]
    async fn agent_diff_detects_iteration_change() {
        let calc = AgentDiffCalculator::new();
        let prev = make_agent_snapshot(1);
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            current_iteration: 2,
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert_eq!(delta.added_iterations, Some(vec![2]));
    }

    #[tokio::test]
    async fn agent_apply_delta_updates_iteration() {
        let calc = AgentDiffCalculator::new();
        let base = make_agent_snapshot(1);
        let delta = wf_types::checkpoint::agent::AgentCheckpointDelta {
            added_messages: None,
            added_message_base_seq: None,
            added_iterations: Some(vec![2, 3]),
            status_change: Some(wf_types::checkpoint::FieldChange {
                from: Some("running".to_string()),
                to: Some("completed".to_string()),
            }),
            other_changes: None,
        };

        let result = calc.apply_delta(&base, &delta).await.unwrap();
        assert_eq!(result.current_iteration, 3);
        assert_eq!(result.status, "completed");
    }

    #[tokio::test]
    async fn agent_other_changes_round_trip() {
        use wf_types::checkpoint::agent::AgentStateSnapshot;

        let calc = AgentDiffCalculator::new();
        let prev = make_agent_snapshot(1);
        let curr = AgentStateSnapshot {
            tool_call_count: 5,
            error_records: Some(vec![serde_json::json!({"type": "tool_error"})]),
            stream_message: Some("partial".to_string()),
            pending_tool_call_ids: Some(vec!["tc-1".to_string()]),
            messages: Some(vec![make_message("m1", "hi")]),
            conversation_view: Some(wf_types::message::MessageView::Compacted {
                segments: vec![wf_types::message::ViewSegment::Summary {
                    message_id: "s1".to_string(),
                }],
            }),
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert!(delta.other_changes.is_some());

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.tool_call_count, 5);
        assert_eq!(restored.error_records, curr.error_records);
        assert_eq!(restored.stream_message, curr.stream_message);
        assert_eq!(restored.pending_tool_call_ids, curr.pending_tool_call_ids);
        assert_eq!(restored.messages, curr.messages);
        assert_eq!(restored.conversation_view, curr.conversation_view);
    }

    #[tokio::test]
    async fn agent_diff_detects_status_change() {
        let calc = AgentDiffCalculator::new();
        let prev = make_agent_snapshot(1);
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            status: "completed".to_string(),
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert!(delta.status_change.is_some());
        assert!(delta.added_iterations.is_none());
        assert!(delta.added_messages.is_none());
        assert!(delta.other_changes.is_none());
    }

    #[tokio::test]
    async fn agent_message_append_round_trip() {
        let m1 = make_message("m1", "hello");
        let m2 = make_message("m2", "world");
        let m3 = make_message("m3", "third");

        let prev = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![m1.clone(), m2.clone()]),
            message_seq_start: Some(10),
            ..make_agent_snapshot(1)
        };
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![m1.clone(), m2.clone(), m3.clone()]),
            ..prev.clone()
        };

        let calc = AgentDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        // Append-only suffix with its sequence coordinate.
        assert_eq!(delta.added_messages, Some(vec![m3.clone()]));
        assert_eq!(delta.added_message_base_seq, Some(12));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.conversation_snapshot, curr.conversation_snapshot);
    }

    #[tokio::test]
    async fn agent_message_prefix_divergence_full_replacement() {
        let prev = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![make_message("m1", "old")]),
            ..make_agent_snapshot(1)
        };
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![
                make_message("n1", "new"),
                make_message("n2", "also new"),
            ]),
            ..prev.clone()
        };

        let calc = AgentDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        // First id differs on a non-empty history: legacy full replacement.
        assert_eq!(delta.added_message_base_seq, None);
        assert_eq!(delta.added_messages, curr.conversation_snapshot);

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.conversation_snapshot, curr.conversation_snapshot);
    }

    #[tokio::test]
    async fn agent_message_identical_no_delta() {
        let calc = AgentDiffCalculator::new();
        let prev = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![make_message("m1", "hello")]),
            message_seq_start: Some(5),
            ..make_agent_snapshot(1)
        };
        let curr = prev.clone();

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert!(delta.added_messages.is_none());
        assert!(delta.added_message_base_seq.is_none());
        assert!(delta.other_changes.is_none());
    }

    #[tokio::test]
    async fn agent_field_removal_round_trip() {
        let calc = AgentDiffCalculator::new();
        let prev = wf_types::checkpoint::agent::AgentStateSnapshot {
            tool_call_count: 5,
            stream_message: Some("partial".to_string()),
            error_records: Some(vec![serde_json::json!({"type": "tool_error"})]),
            pending_tool_call_ids: Some(vec!["tc-1".to_string()]),
            ..make_agent_snapshot(1)
        };
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            tool_call_count: 0,
            stream_message: None,
            error_records: None,
            pending_tool_call_ids: None,
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        let other = delta.other_changes.as_ref().expect("other changes");
        assert!(other.contains_key("tool_call_count"));
        assert!(other.contains_key("stream_message"));
        assert!(other.contains_key("error_records"));
        assert!(other.contains_key("pending_tool_call_ids"));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored, curr);
    }

    #[tokio::test]
    async fn agent_merge_deltas_equals_sequential_apply() {
        let calc = AgentDiffCalculator::new();
        let base = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![make_message("m1", "hello")]),
            message_seq_start: Some(10),
            ..make_agent_snapshot(1)
        };
        let mid = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![
                make_message("m1", "hello"),
                make_message("m2", "world"),
            ]),
            ..base.clone()
        };
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            status: "completed".to_string(),
            conversation_snapshot: Some(vec![
                make_message("m1", "hello"),
                make_message("m2", "world"),
                make_message("m3", "third"),
            ]),
            ..base.clone()
        };

        let first = calc.calculate_diff(&base, &mid).await.unwrap();
        let second = calc.calculate_diff(&mid, &curr).await.unwrap();
        let merged = calc.merge_deltas(&base, &first, &second).await.unwrap();

        let direct = calc.calculate_diff(&base, &curr).await.unwrap();
        let restored_via_merge = calc.apply_delta(&base, &merged).await.unwrap();
        let restored_direct = calc.apply_delta(&base, &direct).await.unwrap();
        assert_eq!(restored_via_merge, restored_direct);
        assert_eq!(restored_via_merge.status, "completed");
        assert_eq!(
            restored_via_merge.conversation_snapshot,
            Some(vec![
                make_message("m1", "hello"),
                make_message("m2", "world"),
                make_message("m3", "third")
            ])
        );
    }

    #[tokio::test]
    async fn agent_apply_delta_rejects_base_seq_before_start() {
        use crate::error::CheckpointError;
        use wf_types::checkpoint::agent::AgentCheckpointDelta;

        let calc = AgentDiffCalculator::new();
        let base = wf_types::checkpoint::agent::AgentStateSnapshot {
            conversation_snapshot: Some(vec![make_message("m1", "hello")]),
            message_seq_start: Some(10),
            ..make_agent_snapshot(1)
        };
        let delta = AgentCheckpointDelta {
            added_messages: Some(vec![make_message("m2", "world")]),
            added_message_base_seq: Some(5),
            added_iterations: None,
            status_change: None,
            other_changes: None,
        };

        // A base_seq preceding the base start means a mismatched base: the
        // splice must fail loudly instead of truncating the wrong prefix.
        let err = calc.apply_delta(&base, &delta).await.unwrap_err();
        assert!(matches!(err, CheckpointError::Corrupted { .. }));
    }
}
