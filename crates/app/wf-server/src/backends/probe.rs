//! Backend liveness prober: port connect, then health, then info for
//! the server version. One entry's timeout never stalls the round and one
//! entry's failure never touches another entry's status.

use std::time::Duration;

use super::types::{BackendEntry, BackendStatusView};

/// Probe one backend tunnel address with a per-entry timeout. One shared
/// deadline bounds the whole entry (connect, health and info combined),
/// so a slow entry can never stall the round beyond `timeout`. TCP
/// connect comes first so a dead forward fails fast without an HTTP
/// round trip.
pub async fn probe_one(
    entry: &BackendEntry,
    default_local_port: u16,
    timeout: Duration,
) -> BackendStatusView {
    let started = std::time::Instant::now();
    let probed = tokio::time::timeout_at(
        tokio::time::Instant::now() + timeout,
        probe_inner(entry, default_local_port, timeout),
    )
    .await
    .ok()
    .flatten();
    match probed {
        Some(versioned) => BackendStatusView {
            reachable: true,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            version: versioned.version,
            api_version: versioned.api_version,
            last_probe_ms: Some(wf_common::epoch_ms()),
        },
        None => unreachable(),
    }
}

/// Connect, health-check and fetch the server version. Every stage
/// returns `None` on failure; the caller owns the single deadline, so
/// no stage wraps its own timeout here.
async fn probe_inner(
    entry: &BackendEntry,
    default_local_port: u16,
    timeout: Duration,
) -> Option<InfoView> {
    let port = entry.effective_local_port(default_local_port);
    let base_url = entry.local_base_url(default_local_port);
    tcp_connect(port).await.ok()?;
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .ok()?;
    client
        .get(format!("{base_url}/health"))
        .send()
        .await
        .ok()
        .filter(|response| response.status().is_success())?;
    fetch_info(&client, &base_url).await
}

#[derive(Debug, Clone, serde::Deserialize)]
struct InfoView {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    api_version: Option<String>,
}

fn unreachable() -> BackendStatusView {
    BackendStatusView {
        reachable: false,
        latency_ms: None,
        version: None,
        api_version: None,
        last_probe_ms: Some(wf_common::epoch_ms()),
    }
}

async fn tcp_connect(port: u16) -> Result<(), ()> {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .map(|_| ())
        .map_err(|_| ())
}

async fn fetch_info(client: &reqwest::Client, base_url: &str) -> Option<InfoView> {
    let body: serde_json::Value = client
        .get(format!("{base_url}/api/v1/info"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let data = body.get("data")?;
    let version = data
        .get("version")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let api_version = data
        .get("api_version")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    if version.is_none() && api_version.is_none() {
        return None;
    }
    Some(InfoView {
        version,
        api_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hermetic probe tests: loopback only, behavior assertions only.
    // Wall-clock budgets are deliberately absent: under parallel load
    // the scheduler alone can stall any thread, so elapsed-time asserts
    // would reintroduce the flake this test once had.
    fn entry_for(port: u16) -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: Some(port),
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
        }
    }

    /// A port nothing listens on: bind an ephemeral port and drop it.
    async fn closed_port() -> u16 {
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback binds")
            .local_addr()
            .expect("bound addr reports")
            .port()
    }

    #[tokio::test]
    async fn refused_port_is_unreachable() {
        let status = probe_one(
            &entry_for(closed_port().await),
            18080,
            Duration::from_millis(200),
        )
        .await;
        assert!(!status.reachable);
        assert!(status.latency_ms.is_none());
    }

    #[tokio::test]
    async fn blackhole_port_is_unreachable() {
        // Accept and hold: the TCP connect succeeds, the HTTP round trip
        // never answers, and only the shared deadline ends the probe.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback binds");
        let port = listener
            .local_addr()
            .expect("bound addr reports")
            .port();
        tokio::spawn(async move {
            if let Ok((socket, _)) = listener.accept().await {
                let _held = socket;
                std::future::pending::<()>().await;
            }
        });
        let status = probe_one(&entry_for(port), 18080, Duration::from_millis(200)).await;
        assert!(!status.reachable);
    }

    #[tokio::test]
    async fn healthy_stub_is_reachable_with_version() {
        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route(
                "/api/v1/info",
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!({
                        "data": { "version": "9.9.9", "api_version": "v1" },
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback binds");
        let port = listener
            .local_addr()
            .expect("bound addr reports")
            .port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let status = probe_one(&entry_for(port), 18080, Duration::from_secs(5)).await;
        assert!(status.reachable);
        assert_eq!(status.version.as_deref(), Some("9.9.9"));
        assert_eq!(status.api_version.as_deref(), Some("v1"));
    }
}
