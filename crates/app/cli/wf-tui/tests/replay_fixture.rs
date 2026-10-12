// Long-session replay fixture smoke.
//
// Serves a stub `wf-server` (summary, conversation, iteration-history) with
// a 220-message ledger and asserts the replay loader that backs the TUI
// Executions → Enter drill-down reads the whole history back through the
// HTTP client, paged newest-first like the production loading flow.

use std::sync::Arc;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use tokio::sync::Mutex;
use wf_tui::replay::{replay_scrollack, replay_scrollack_page};
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
        thinking_signature: None,
        metadata: None,
    }
}

fn ledger() -> Vec<Message> {
    (0..MESSAGE_COUNT)
        .map(|i| {
            let (role, text) = if i % 2 == 0 {
                (MessageRole::User, format!("user message {i}"))
            } else {
                (MessageRole::Assistant, format!("assistant reply {i}"))
            };
            make_message(&format!("m{i}"), role, &text, 1001 + i)
        })
        .collect()
}

fn summary() -> serde_json::Value {
    serde_json::json!({
        "id": SESSION_ID,
        "status": "completed",
        "current_iteration": 2,
        "tool_call_count": 0,
        "start_time": 1000,
        "end_time": 1000 + MESSAGE_COUNT,
    })
}

fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"success": false, "data": null, "error": {"code": "NOT_FOUND", "message": "missing"}})),
    )
}

async fn stub_server(messages: Vec<Message>) -> String {
    let shared = Arc::new(Mutex::new(messages));
    let for_conversation = Arc::clone(&shared);
    let app = Router::new()
        .route(
            "/api/v1/agent-loops/{id}/summary",
            get(|Path(id): Path<String>| async move {
                if id == SESSION_ID {
                    Ok(Json(
                        serde_json::json!({"success": true, "data": summary(), "error": null}),
                    ))
                } else {
                    Err(not_found())
                }
            }),
        )
        .route(
            "/api/v1/agent-loops/{id}/conversation",
            get(move |Path(id): Path<String>| {
                let shared = Arc::clone(&for_conversation);
                async move {
                    if id != SESSION_ID {
                        return Err(not_found());
                    }
                    let messages = shared.lock().await.clone();
                    let total = messages.len();
                    Ok(Json(serde_json::json!({
                        "success": true,
                        "data": {"items": messages, "limit": 5000, "offset": 0, "total": total},
                        "error": null,
                    })))
                }
            }),
        )
        .route(
            "/api/v1/agent-loops/{id}/iteration-history",
            get(|Path(id): Path<String>| async move {
                if id == SESSION_ID {
                    Ok(Json(
                        serde_json::json!({"success": true, "data": [], "error": null}),
                    ))
                } else {
                    Err(not_found())
                }
            }),
        )
        .route(
            "/api/v1/messages/by-execution/{id}",
            get(|| async {
                Json(serde_json::json!({"success": true, "data": {"items": [], "limit": 0, "offset": 0, "total": 0}, "error": null}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn test_client(base: &str) -> wf_client::WfClient {
    wf_client::WfClient::new(wf_client::ClientConfig::new(base))
}

#[tokio::test]
async fn stub_server_replays_whole_long_session() {
    let base = stub_server(ledger()).await;
    let client = test_client(&base);

    // The TUI loading flow fetches through this exact function; a long
    // session must come back whole and ordered, oldest first.
    let lines = replay_scrollack(&client, SESSION_ID).await.unwrap();
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
async fn stub_server_replay_pages_newest_first() {
    let base = stub_server(ledger()).await;
    let client = test_client(&base);

    let first = replay_scrollack_page(&client, SESSION_ID, None, 2)
        .await
        .unwrap();
    assert!(first.has_more);
    let texts: Vec<String> = first.lines.iter().flat_map(|l| l.raw_lines(200)).collect();
    let joined = texts.join("\n");
    assert!(joined.contains("assistant reply 219"), "tail page holds newest");

    let older = replay_scrollack_page(&client, SESSION_ID, first.next_before, 2)
        .await
        .unwrap();
    let older_texts: Vec<String> = older
        .lines
        .iter()
        .flat_map(|l| l.raw_lines(200))
        .collect();
    assert!(
        older_texts.iter().any(|t| t.contains("user message")),
        "older page holds earlier history"
    );
}

#[tokio::test]
async fn stub_server_replay_missing_session_fails_cleanly() {
    let base = stub_server(ledger()).await;
    let client = test_client(&base);

    // The error branch that renders "✗ replay failed" in the session screen
    // instead of leaving the loading placeholder up forever.
    let err = replay_scrollack(&client, "ghost-session").await.unwrap_err();
    assert!(err.to_string().contains("ghost-session"), "{err}");
}
