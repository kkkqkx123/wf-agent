//! Backend liveness prober: port connect, then health, then info for
//! the server version. One entry's timeout never stalls the round and one
//! entry's failure never touches another entry's status.

use std::time::Duration;

use super::types::{BackendEntry, BackendStatusView};

/// Probe one backend tunnel address with a per-entry timeout. TCP connect
/// comes first so a dead forward fails fast without an HTTP round trip.
pub async fn probe_one(
    entry: &BackendEntry,
    default_local_port: u16,
    timeout: Duration,
) -> BackendStatusView {
    let port = entry.effective_local_port(default_local_port);
    let base_url = entry.local_base_url(default_local_port);
    let started = std::time::Instant::now();
    if tokio::time::timeout(timeout, tcp_connect(port))
        .await
        .is_err()
    {
        return unreachable();
    }
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let health = tokio::time::timeout(timeout, client.get(format!("{base_url}/health")).send())
        .await
        .ok()
        .and_then(|result| result.ok())
        .filter(|response| response.status().is_success());
    if health.is_none() {
        return unreachable();
    }
    let versioned = fetch_info(&base_url, timeout).await;
    BackendStatusView {
        reachable: true,
        latency_ms: Some(started.elapsed().as_millis() as u64),
        version: versioned.as_ref().and_then(|view| view.version.clone()),
        api_version: versioned.as_ref().and_then(|view| view.api_version.clone()),
        last_probe_ms: Some(wf_common::epoch_ms()),
    }
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

async fn fetch_info(base_url: &str, timeout: Duration) -> Option<InfoView> {
    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
    let body: serde_json::Value = tokio::time::timeout(
        timeout,
        client.get(format!("{base_url}/api/v1/info")).send(),
    )
    .await
    .ok()?
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

    #[tokio::test]
    async fn dead_port_is_unreachable_without_stall() {
        let entry = BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: Some(1),
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
        };
        let started = std::time::Instant::now();
        let status = probe_one(&entry, 18080, Duration::from_millis(200)).await;
        assert!(!status.reachable);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
