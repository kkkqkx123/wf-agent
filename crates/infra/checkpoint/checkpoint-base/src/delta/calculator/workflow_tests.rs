#[cfg(test)]
mod tests {
    use crate::delta::calculator::test_support::{make_message, make_workflow_snapshot};
    use crate::delta::calculator::workflow::*;
    use crate::delta::DiffCalculator;
    use wf_types::checkpoint::workflow::{OperationState, WorkflowExecutionStateSnapshot};

    async fn round_trip(
        prev: &WorkflowExecutionStateSnapshot,
        curr: &WorkflowExecutionStateSnapshot,
    ) -> WorkflowExecutionStateSnapshot {
        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(prev, curr).await.unwrap();
        calc.apply_delta(prev, &delta).await.unwrap()
    }

    #[tokio::test]
    async fn workflow_diff_detects_status_change() {
        let calc = WorkflowDiffCalculator::new();
        let prev = make_workflow_snapshot();
        let curr = WorkflowExecutionStateSnapshot {
            status: "completed".to_string(),
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert!(delta.status_change.is_some());
        assert!(delta.current_node_change.is_none());
    }

    #[tokio::test]
    async fn workflow_apply_delta_updates_status() {
        let calc = WorkflowDiffCalculator::new();
        let base = make_workflow_snapshot();
        let delta = wf_types::checkpoint::workflow::WorkflowCheckpointDelta {
            added_messages: None,
            modified_messages: None,
            deleted_message_indices: None,
            added_variables: None,
            modified_variables: None,
            message_contexts: None,
            added_node_results: None,
            modified_node_results: None,
            status_change: Some(wf_types::checkpoint::FieldChange {
                from: Some("running".to_string()),
                to: Some("completed".to_string()),
            }),
            current_node_change: Some(wf_types::checkpoint::FieldChange {
                from: Some("node-4".to_string()),
                to: Some("node-5".to_string()),
            }),
            other_changes: None,
        };

        let result = calc.apply_delta(&base, &delta).await.unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.current_node_id, Some("node-5".to_string()));
    }

    #[tokio::test]
    async fn workflow_message_add_modify_delete_round_trip() {
        let m1 = make_message("m1", "hello");
        let m2 = make_message("m2", "world");
        let m3 = make_message("m3", "third");

        let prev = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2.clone()]),
            ..make_workflow_snapshot()
        };

        let m2_modified = make_message("m2", "world-modified");
        let curr = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2_modified.clone(), m3.clone()]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();

        assert_eq!(delta.added_messages, Some(vec![m3.clone()]));
        assert_eq!(delta.modified_messages, Some(vec![m2_modified.clone()]));
        assert!(delta.deleted_message_indices.is_none());

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.messages, curr.messages);
    }

    #[tokio::test]
    async fn workflow_message_delete_round_trip() {
        let m1 = make_message("m1", "hello");
        let m2 = make_message("m2", "world");

        let prev = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2.clone()]),
            ..make_workflow_snapshot()
        };
        let curr = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone()]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert_eq!(delta.deleted_message_indices, Some(vec![1]));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.messages, curr.messages);
    }

    #[tokio::test]
    async fn workflow_variables_add_modify_delete_round_trip() {
        use std::collections::HashMap;

        let mut prev_vars = HashMap::new();
        prev_vars.insert("a".to_string(), serde_json::json!(1));
        prev_vars.insert("b".to_string(), serde_json::json!("keep"));
        let prev = WorkflowExecutionStateSnapshot {
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: prev_vars,
            },
            ..make_workflow_snapshot()
        };

        let mut curr_vars = HashMap::new();
        curr_vars.insert("a".to_string(), serde_json::json!(2));
        curr_vars.insert("b".to_string(), serde_json::json!("keep"));
        curr_vars.insert("c".to_string(), serde_json::json!("new"));
        let curr = WorkflowExecutionStateSnapshot {
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: curr_vars,
            },
            ..prev.clone()
        };

        let restored = round_trip(&prev, &curr).await;
        assert_eq!(restored.variable_state, curr.variable_state);
    }

    #[tokio::test]
    async fn workflow_other_changes_round_trip() {
        let prev = make_workflow_snapshot();
        let curr = WorkflowExecutionStateSnapshot {
            input: Some(serde_json::json!({"prompt": "hello"})),
            output: Some(serde_json::json!({"result": 42})),
            fork_join_context: Some(serde_json::json!({"forkId": "f1"})),
            active_operations: Some(vec![OperationState {
                r#type: "execute".to_string(),
                operation_id: "op-1".to_string(),
                node_id: Some("node-1".to_string()),
                started_at: 123,
                progress: None,
                partial_result: None,
            }]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        let other = delta.other_changes.as_ref().unwrap();
        assert!(other.contains_key("input"));
        assert!(other.contains_key("output"));
        assert!(other.contains_key("fork_join_context"));
        assert!(other.contains_key("active_operations"));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.input, curr.input);
        assert_eq!(restored.output, curr.output);
        assert_eq!(restored.fork_join_context, curr.fork_join_context);
        assert_eq!(restored.active_operations, curr.active_operations);
    }

    #[tokio::test]
    async fn workflow_field_removal_round_trip() {
        let prev = WorkflowExecutionStateSnapshot {
            input: Some(serde_json::json!({"prompt": "hello"})),
            messages: Some(vec![make_message("m1", "hi")]),
            ..make_workflow_snapshot()
        };
        let curr = WorkflowExecutionStateSnapshot {
            input: None,
            ..prev.clone()
        };

        let restored = round_trip(&prev, &curr).await;
        assert_eq!(restored.input, None);
        assert_eq!(restored.messages, Some(vec![make_message("m1", "hi")]));
    }

    #[tokio::test]
    async fn merge_deltas_equals_sequential_apply() {
        let calc = WorkflowDiffCalculator::new();
        let base = make_workflow_snapshot();
        let mid = WorkflowExecutionStateSnapshot {
            status: "running".to_string(),
            messages: Some(vec![make_message("m1", "hello")]),
            ..base.clone()
        };
        let curr = WorkflowExecutionStateSnapshot {
            status: "completed".to_string(),
            messages: Some(vec![
                make_message("m1", "hello"),
                make_message("m2", "world"),
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
            restored_via_merge.messages,
            Some(vec![
                make_message("m1", "hello"),
                make_message("m2", "world")
            ])
        );
    }
}
