//! End-to-end behaviour of a tree session over a mock provider.
//!
//! The interesting properties are not "the model answered" but what the tree
//! records: which answer is fresh, what an in-place edit does to the answers
//! below it, and whether a branch edit leaves the original branch alone.

use std::sync::Arc;
use std::time::Duration;

use wf_execution_shared::conversation_tree;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_resource::registry::ResourceRegistries;
use wf_storage::context::StorageContext;

use super::*;
use crate::infra::context::ApiContext;

fn make_ctx(replies: &[&str]) -> Arc<ApiContext> {
    let mock = Arc::new(MockLlmClient::new());
    for reply in replies {
        mock.script(LlmResponseSpec::text(*reply));
    }
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock);
    Arc::new(ApiContext::from_runtime_parts(
        Arc::new(StorageContext::new_memory()),
        Arc::new(ResourceRegistries::new()),
        Arc::new(wf_core::EventBus::new(64)),
        gateway,
        Arc::new(wf_tools::create_default_tool_registry()),
        None,
    ))
}

async fn session(ctx: &Arc<ApiContext>) -> String {
    create_session(ctx, Some("mock".to_string()), None)
        .await
        .expect("session created")
        .session_id
}

/// Submit one turn and wait until its generation settles.
async fn turn(ctx: &Arc<ApiContext>, session_id: &str, text: &str) -> TurnHandle {
    let handle = submit_turn(
        ctx.clone(),
        TurnRequest {
            session_id: session_id.to_string(),
            profile_id: Some("mock".to_string()),
            message: text.to_string(),
            ..TurnRequest::default()
        },
    )
    .await
    .expect("turn accepted");
    wait_until(|| {
        let session_id = session_id.to_string();
        let ctx = ctx.clone();
        async move {
            runtime::runtime_for(&session_id).inflight().is_none()
                && dirty_list(&ctx, &session_id, None)
                    .await
                    .map(|dirty| dirty.is_empty())
                    .unwrap_or(false)
        }
    })
    .await;
    handle
}

/// Poll `condition` until it holds. The driver runs detached, so a test reads
/// the tree the way a client does: by waiting for the state to settle.
async fn wait_until<F, Fut>(mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..400 {
        if condition().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition did not settle");
}

fn surgery(session_id: &str) -> SurgeryRequest {
    SurgeryRequest {
        session_id: session_id.to_string(),
        if_version: None,
    }
}

#[tokio::test]
async fn a_turn_lands_in_the_tree_and_stores_both_messages() {
    let ctx = make_ctx(&["hello there"]);
    let session_id = session(&ctx).await;
    let handle = turn(&ctx, &session_id, "hi").await;

    let snapshot = snapshot(&ctx, &session_id, None).await.expect("snapshot");
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == handle.node_id)
        .expect("node is in the tree");
    assert_eq!(node.reply.as_ref().map(|reply| reply.state), Some("kept"));
    assert_eq!(node.parent, handle.parent);
    assert!(!node.dirty);

    let path = path_messages(&ctx, &session_id, None)
        .await
        .expect("path readable");
    assert_eq!(path.len(), 2, "user + reply: {path:?}");
    assert_eq!(path[0].role, "user");
    assert_eq!(path[1].role, "reply");
    assert!(path[1].message.text_content().contains("hello there"));
}

#[tokio::test]
async fn an_in_place_edit_makes_every_answer_below_it_stale() {
    let ctx = make_ctx(&["first answer", "second answer"]);
    let session_id = session(&ctx).await;
    let first = turn(&ctx, &session_id, "question one").await;
    let second = turn(&ctx, &session_id, "question two").await;

    assert!(dirty_list(&ctx, &session_id, None)
        .await
        .expect("dirty")
        .is_empty());

    write_user_content(&ctx, surgery(&session_id), &first.node_id, "question one, fixed")
        .await
        .expect("in-place edit");

    let dirty = dirty_list(&ctx, &session_id, None).await.expect("dirty");
    let stale: Vec<&str> = dirty.entries.iter().map(|e| e.node_id.as_str()).collect();
    assert!(stale.contains(&first.node_id.as_str()), "{stale:?}");
    assert!(stale.contains(&second.node_id.as_str()), "{stale:?}");
    assert_eq!(dirty.entries[0].node_id, first.node_id, "path order");
}

#[tokio::test]
async fn a_branch_edit_leaves_the_original_branch_fresh() {
    let ctx = make_ctx(&["first answer"]);
    let session_id = session(&ctx).await;
    let first = turn(&ctx, &session_id, "question one").await;

    let branch = edit_branch(&ctx, surgery(&session_id), &first.node_id, "another phrasing")
        .await
        .expect("branch edit");

    let snapshot = snapshot(&ctx, &session_id, None).await.expect("snapshot");
    assert_eq!(snapshot.nodes.len(), 2, "a sibling, not a rewrite");
    let original = snapshot
        .nodes
        .iter()
        .find(|node| node.id == first.node_id)
        .expect("original node");
    assert!(!original.dirty, "the untouched branch stays fresh");
    // The new branch waits for its own answer, which is the only staleness.
    let dirty = dirty_list(&ctx, &session_id, None).await.expect("dirty");
    assert_eq!(dirty.entries.len(), 1);
    assert_eq!(dirty.entries[0].node_id, branch.node_id);
}

#[tokio::test]
async fn a_single_step_regenerates_one_answer_at_a_time() {
    let ctx = make_ctx(&["first answer", "second answer", "first again"]);
    let session_id = session(&ctx).await;
    let first = turn(&ctx, &session_id, "question one").await;
    turn(&ctx, &session_id, "question two").await;

    write_user_content(&ctx, surgery(&session_id), &first.node_id, "question one, fixed")
        .await
        .expect("in-place edit");
    assert_eq!(
        dirty_list(&ctx, &session_id, None)
            .await
            .expect("dirty")
            .len(),
        2
    );

    let handle = start_step(
        ctx.clone(),
        StepRequest {
            session_id: session_id.clone(),
            mode: StepMode::Single,
            ..StepRequest::default()
        },
    )
    .await
    .expect("step accepted");
    assert_eq!(handle.plan.len(), 1, "single mode plans one node");

    wait_until(|| {
        let session_id = session_id.clone();
        let ctx = ctx.clone();
        async move {
            runtime::runtime_for(&session_id).inflight().is_none()
                && dirty_list(&ctx, &session_id, None)
                    .await
                    .map(|dirty| dirty.len() == 1)
                    .unwrap_or(false)
        }
    })
    .await;
}

#[tokio::test]
async fn detaching_with_the_prefix_keeps_the_subtree_fresh() {
    let ctx = make_ctx(&["first answer", "second answer"]);
    let session_id = session(&ctx).await;
    let first = turn(&ctx, &session_id, "question one").await;
    let second = turn(&ctx, &session_id, "question two").await;

    let detached = detach_subtree(&ctx, surgery(&session_id), &second.node_id, true, None)
        .await
        .expect("detach");
    assert_eq!(detached.nodes, 1);

    // The graft keeps the ancestors, so the moved answer is still fresh.
    let grafts = list_grafts(&ctx, &session_id).await.expect("grafts");
    assert_eq!(grafts.len(), 1);
    let graft = store::load_graft(&ctx, &detached.graft_id)
        .await
        .expect("graft document");
    let index = conversation_tree::graft_index(&graft);
    let moved = graft.node(&second.node_id).expect("moved node");
    assert!(
        !index.is_dirty(moved),
        "a carried prefix keeps the answer fresh"
    );

    // Grafting it back under its original parent restores the session.
    attach_graft(
        &ctx,
        surgery(&session_id),
        &detached.graft_id,
        &first.node_id,
        None,
    )
    .await
    .expect("attach");
    let dirty = dirty_list(&ctx, &session_id, None).await.expect("dirty");
    assert!(dirty.is_empty(), "round trip stays clean: {dirty:?}");
}

#[tokio::test]
async fn a_rewritten_answer_becomes_the_new_baseline() {
    let ctx = make_ctx(&["machine answer", "second answer"]);
    let session_id = session(&ctx).await;
    let first = turn(&ctx, &session_id, "question one").await;
    turn(&ctx, &session_id, "question two").await;

    write_reply_body(&ctx, surgery(&session_id), &first.node_id, "human answer")
        .await
        .expect("rewrite");

    let path = path_messages(&ctx, &session_id, None).await.expect("path");
    assert!(path[1].message.text_content().contains("human answer"));
    let snapshot = snapshot(&ctx, &session_id, None).await.expect("snapshot");
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == first.node_id)
        .expect("node");
    assert_eq!(node.reply.as_ref().map(|reply| reply.origin), Some("human"));
    assert_eq!(
        node.reply.as_ref().map(|reply| reply.superseded.len()),
        Some(1),
        "the machine answer is archived, not lost"
    );
}
