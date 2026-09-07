// Long-session replay fixture smoke.
//
// Seeds a persisted sqlite store through the public write path (an agent
// execution record plus entity messages — the same stores a real headless
// run fills) and asserts the replay loader that backs the TUI Executions →
// Enter drill-down reads the whole history back without truncation. This is
// the CI-side of the "fixture library" replay acceptance: a scripted run on
// a real board can re-use the same sqlite layout with `--storage sqlite:<db>`.

use std::sync::Arc;

use wf_api::infra::context::ApiContext;
use wf_api::BaseStorageAdapter;
use wf_cli::replay::replay_scrollack;
use wf_resource::registry::ResourceRegistries;
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_storage::context::StorageContext;
use wf_types::message::{Message, MessageContentValue, MessageRole};

const SESSION_ID: &str = "exec-long-1";
const MESSAGE_COUNT: i64 = 220;

fn make_message(id: &str, role: MessageRole, text: &str, ts: i64) -> Message {
    Message {
        id: id.into(),
        role,
        content: MessageContentValue::Text(text.to_string()),
        timestamp: ts,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

async fn make_sqlite_ctx(db_path: &std::path::Path) -> Arc<ApiContext> {
    let storage = StorageContext::new_sqlite(db_path.to_str().unwrap())
        .await
        .unwrap();
    Arc::new(ApiContext::new(
        storage,
        Arc::new(ResourceRegistries::new()),
        Arc::new(ResourcePluginRegistry::new()),
    ))
}

async fn seed_long_session(ctx: &ApiContext) {
    // The persisted agent-execution row makes `agent_loop_registry::summary`
    // resolve — the same validation the replay entry point runs.
    let record = wf_types::AgentExecution {
        id: wf_types::Id::from(SESSION_ID.to_string()),
        definition_id: wf_types::Id::from("agent-x".to_string()),
        status: wf_types::ExecutionStatus::Completed,
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: None,
        started_at: 1000,
        completed_at: Some(1000 + MESSAGE_COUNT),
        error: None,
        context: None,
    };
    ctx.storage.agent_execution.save(&record).await.unwrap();

    for i in 0..MESSAGE_COUNT {
        let (role, text) = if i % 2 == 0 {
            (MessageRole::User, format!("user message {i}"))
        } else {
            (MessageRole::Assistant, format!("assistant reply {i}"))
        };
        wf_api::entity::message::add_message(
            ctx,
            SESSION_ID,
            Some(SESSION_ID),
            make_message(&format!("m{i}"), role, &text, 1001 + i),
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn sqlite_fixture_replays_whole_long_session() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("replay-long.sqlite");
    let ctx = make_sqlite_ctx(&db_path).await;
    seed_long_session(&ctx).await;

    // The TUI loading flow fetches through this exact function; a long
    // session must come back whole and ordered, oldest first.
    let lines = replay_scrollack(&ctx, SESSION_ID).await.unwrap();
    assert!(lines.len() >= MESSAGE_COUNT as usize);

    let texts: Vec<String> = lines.iter().flat_map(|l| l.raw_lines(200)).collect();
    let joined = texts.join("\n");
    assert!(
        joined.contains("user message 0"),
        "earliest history present"
    );
    assert!(
        joined.contains("assistant reply 219"),
        "latest history present"
    );
}

#[tokio::test]
async fn sqlite_fixture_replay_missing_session_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("replay-long.sqlite");
    let ctx = make_sqlite_ctx(&db_path).await;
    seed_long_session(&ctx).await;

    // The error branch that renders "✗ replay failed" in the session screen
    // instead of leaving the loading placeholder up forever.
    let err = replay_scrollack(&ctx, "ghost-session").await.unwrap_err();
    assert!(matches!(
        err,
        wf_api::infra::error::ApiError::ExecutionNotFound { .. }
    ));
}
