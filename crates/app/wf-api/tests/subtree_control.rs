//! Subtree control tests: pause/resume/stop/cancel over a live hierarchy
//! linked through the hierarchy manager.

use std::sync::Arc;

use wf_api::ApiContext;
use wf_core::registry::MutableRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_resource::registry::ResourceRegistries;
use wf_storage::context::StorageContext;
use wf_workflow::entity::WorkflowExecutionEntity;

fn api_context() -> ApiContext {
    ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    )
}

async fn live_hierarchy(
    ctx: &ApiContext,
) -> (Arc<WorkflowExecutionEntity>, Arc<WorkflowExecutionEntity>) {
    let root = Arc::new(WorkflowExecutionEntity::new("root".into(), "wf-1".into()));
    let child_manager = WorkflowExecutionEntity::hierarchy_manager(&root)
        .derive_child(
            "child".into(),
            wf_types::execution::ExecutionType::Workflow,
            None,
        )
        .expect("derive");
    let child = Arc::new(
        WorkflowExecutionEntity::new("child".into(), "wf-1".into())
            .with_hierarchy_manager(child_manager),
    );
    root.state.write().await.start().expect("root starts");
    child.state.write().await.start().expect("child starts");
    ctx.workflow_executions
        .register_or_replace("root".into(), root.clone());
    ctx.workflow_executions
        .register_or_replace("child".into(), child.clone());
    (root, child)
}

#[tokio::test]
async fn cancel_subtree_stops_parent_and_child() {
    let ctx = api_context();
    let (root, child) = live_hierarchy(&ctx).await;

    ctx.cancel_subtree("root").await.expect("cancel succeeds");

    assert!(root.is_cancelled(), "parent stopped");
    assert!(child.is_cancelled(), "manager-linked child stopped");
}

#[tokio::test]
async fn pause_and_resume_subtree_moves_both() {
    let ctx = api_context();
    let (root, child) = live_hierarchy(&ctx).await;

    ctx.pause_subtree("root").await.expect("pause succeeds");
    assert!(root.is_paused());
    assert!(child.is_paused());

    ctx.resume_subtree("root").await.expect("resume succeeds");
    assert!(root.is_running());
    assert!(child.is_running());
}

#[tokio::test]
async fn subtree_of_unknown_root_is_empty_but_ok() {
    let ctx = api_context();
    assert!(ctx.execution_subtree("missing").is_empty());
    ctx.stop_subtree("missing")
        .await
        .expect("no members, no error");
}
