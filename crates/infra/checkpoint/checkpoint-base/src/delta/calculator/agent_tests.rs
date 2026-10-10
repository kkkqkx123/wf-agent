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
            conversation_view: Some(wf_types::message::MessageView::Compressed {
                summary: Box::new(make_message("s1", "summary")),
                tail_begin: 1,
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
}
