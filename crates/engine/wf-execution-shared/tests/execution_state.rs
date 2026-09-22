use wf_execution_shared::ExecutionStateManager;
use wf_types::{AgentExecution, ExecutionStatus, WorkflowExecution};

fn workflow_record(id: &str) -> WorkflowExecution {
    WorkflowExecution {
        id: id.to_string(),
        workflow_id: "wf-1".to_string(),
        workflow_version: None,
        status: ExecutionStatus::Running,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: None,
        started_at: 0,
        completed_at: None,
        error: None,
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    }
}

fn agent_record(id: &str) -> AgentExecution {
    AgentExecution {
        id: id.to_string(),
        definition_id: "agent-def-1".to_string(),
        status: ExecutionStatus::Running,
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: None,
        started_at: 0,
        completed_at: None,
        error: None,
        context: None,
    }
}

#[tokio::test]
async fn unwired_workflow_persist_is_noop() {
    let manager = ExecutionStateManager::new();
    manager.persist_workflow(&workflow_record("exec-1")).await;
}

#[tokio::test]
async fn unwired_workflow_status_update_is_noop() {
    let manager = ExecutionStateManager::default();
    manager
        .update_workflow_status("missing-exec", &ExecutionStatus::Completed)
        .await;
}

#[tokio::test]
async fn unwired_agent_persist_is_noop() {
    let manager = ExecutionStateManager::new();
    manager.persist_agent(&agent_record("agent-1")).await;
}

#[tokio::test]
async fn unwired_agent_status_update_is_noop() {
    let manager = ExecutionStateManager::default();
    manager
        .update_agent_status("missing-agent", &ExecutionStatus::Failed)
        .await;
}
