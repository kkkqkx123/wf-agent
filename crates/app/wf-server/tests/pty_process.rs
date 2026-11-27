//! Real PTY child-process coverage: the router creates a child, streams its
//! output, accepts input, replays from a cursor, and reaps the child when the
//! server handle shuts down. One test lives in this binary because the PTY
//! manager is process-wide and shutdown affects every session.

#![cfg(all(unix, feature = "backends", feature = "pty"))]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_server::directory::{DirectoryConfig, DirectoryContext};
use wf_server::{serve_api_with_config, ServerConfig};
use wf_storage::context::StorageContext;

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct ChildGuard {
    pid: Option<u32>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid.take() {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
        }
    }
}

fn pid_alive(pid: u32) -> bool {
    let text = pid.to_string();
    Command::new("kill")
        .args(["-0", text.as_str()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

async fn wait_pid_gone(pid: u32) -> bool {
    for _ in 0..60 {
        if !pid_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

async fn next_frame(socket: &mut Socket) -> WsMessage {
    timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("ws frame within timeout")
        .expect("ws stream open")
        .expect("ws frame is valid")
}

async fn read_until_text(socket: &mut Socket, marker: &str) -> String {
    for _ in 0..16 {
        match next_frame(socket).await {
            WsMessage::Text(text) if text.contains(marker) => return text.to_string(),
            WsMessage::Text(_) | WsMessage::Binary(_) | WsMessage::Ping(_) | WsMessage::Pong(_) => {
                continue;
            }
            other => panic!("unexpected frame while waiting for {marker}: {other:?}"),
        }
    }
    panic!("marker not found within frame budget: {marker}")
}

#[tokio::test]
async fn pty_child_streams_input_and_is_reaped_on_shutdown() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let worker = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../e2e/fixtures/process-lifecycle/pty-worker.sh");
    assert!(worker.is_file(), "PTY worker fixture must be present");

    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    let dirs = Arc::new(
        DirectoryContext::with_default(DirectoryConfig::default(), workspace.path())
            .expect("directory scope"),
    );
    let mut config = ServerConfig::default();
    config.bind_addr = "127.0.0.1:0".parse().expect("loopback bind address");
    config.shutdown_drain_secs = 0;

    let handle = serve_api_with_config(ctx, &config, dirs)
        .await
        .expect("server should bind");
    let base = format!("http://{}", handle.addr());
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "cwd": workspace.path().to_string_lossy().into_owned(),
        "command": worker.to_string_lossy().into_owned(),
        "rows": 24,
        "cols": 80,
        "title": "e2e-pty",
        "title_source": "default",
        "persistent": true,
    });
    let created = client
        .post(format!("{base}/api/v1/pty"))
        .json(&body)
        .send()
        .await
        .expect("create request")
        .error_for_status()
        .expect("create response")
        .json::<Value>()
        .await
        .expect("create envelope");
    assert_eq!(created["success"], true, "create envelope must succeed");
    let id = created["data"]["id"].as_str().expect("pty id").to_string();
    let pid = created["data"]["pid"].as_u64().expect("pty pid") as u32;
    assert_eq!(created["data"]["status"], "running");
    assert!(pid_alive(pid), "PTY child must be alive after create");
    let mut guard = ChildGuard { pid: Some(pid) };

    let (mut first, _) = connect_async(format!("ws://{}/api/v1/pty/{id}/connect", handle.addr()))
        .await
        .expect("first websocket connect");
    let mut saw_ready = false;
    let mut cursor = None;
    for _ in 0..16 {
        match next_frame(&mut first).await {
            WsMessage::Text(text) if text.contains("PTY_READY") => saw_ready = true,
            WsMessage::Binary(bytes) => {
                let meta: Value =
                    serde_json::from_slice(&bytes[1..]).expect("initial cursor frame");
                cursor = meta["cursor"].as_i64();
                assert!(meta["replay_start"].is_number(), "replay start is present");
            }
            WsMessage::Text(_) | WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected initial frame: {other:?}"),
        }
        if saw_ready && cursor.is_some() {
            break;
        }
    }
    assert!(saw_ready, "ready marker must reach the websocket");
    let initial_cursor = cursor.expect("initial cursor");

    first
        .send(WsMessage::Text("ping\n".into()))
        .await
        .expect("write to PTY");
    let echoed = read_until_text(&mut first, "PTY_ECHO:ping").await;
    assert!(echoed.contains("PTY_ECHO:ping"));

    let (mut replay, _) = connect_async(format!("ws://{}/api/v1/pty/{id}/connect", handle.addr()))
        .await
        .expect("replay websocket connect");
    let mut replay_ready = false;
    let mut replay_meta = None;
    for _ in 0..16 {
        match next_frame(&mut replay).await {
            WsMessage::Text(text) if text.contains("PTY_READY") => replay_ready = true,
            WsMessage::Binary(bytes) => {
                let meta: Value = serde_json::from_slice(&bytes[1..]).expect("replay cursor frame");
                replay_meta = meta["cursor"].as_i64();
            }
            WsMessage::Text(_) | WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected replay frame: {other:?}"),
        }
        if replay_ready && replay_meta.is_some() {
            break;
        }
    }
    assert!(replay_ready, "replay must contain the ready marker");
    let resume_cursor = replay_meta.expect("replay cursor");
    assert!(resume_cursor >= initial_cursor);
    let _ = replay.close(None).await;

    let (mut resumed, _) = connect_async(format!(
        "ws://{}/api/v1/pty/{id}/connect?cursor={resume_cursor}",
        handle.addr()
    ))
    .await
    .expect("resumed websocket connect");
    for _ in 0..16 {
        match next_frame(&mut resumed).await {
            WsMessage::Text(text) => {
                assert!(
                    !text.contains("PTY_READY"),
                    "cursor must not replay consumed output"
                );
            }
            WsMessage::Binary(_) => break,
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected resumed frame: {other:?}"),
        }
    }
    let _ = resumed.close(None).await;
    let _ = first.close(None).await;

    handle.shutdown().await;
    assert!(
        wait_pid_gone(pid).await,
        "PTY child must not survive shutdown"
    );
    guard.pid = None;
    assert!(
        wf_api::pty::get_pty_session(&id).is_err(),
        "shutdown must clear the session registry"
    );
}
