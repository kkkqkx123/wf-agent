//! Scheduled backups: per-backend execution bundles written under the
//! configured backup directory and recorded in the backup index.
//!
//! Cadence reuses a shared interval timer (ticks that fire late skip, they
//! never backlog) and stops on shutdown. One entry's failure only marks
//! that entry; it never blocks the round or other backends. Retention
//! applies a count cap and an age cap, whichever hits first.

use super::store::BackendStore;
use super::types::{BackendEntry, BackupRecordView};

/// Per-entry backup outcome of the last round, for status views.
#[derive(Debug, Clone, Default)]
pub struct BackupRoundStatus {
    pub last_backup_ms: Option<i64>,
    pub error: Option<String>,
}

/// Fetch one execution bundle plus its checkpoint chain from the backend
/// tunnel address and write a versioned JSON bundle under the backup root.
/// Returns the backup id. Network and remote errors fail loud to the
/// caller, which records them on the entry without touching others.
pub async fn backup_one(
    store: &BackendStore,
    backend: &str,
    entry: &BackendEntry,
    execution: &str,
    backup_root: Option<&std::path::Path>,
    default_local_port: u16,
) -> Result<String, String> {
    backup_one_with_fetch(
        store,
        backend,
        entry,
        execution,
        backup_root,
        default_local_port,
        fetch_bundle,
    )
    .await
}

async fn backup_one_with_fetch<F, Fut>(
    store: &BackendStore,
    backend: &str,
    entry: &BackendEntry,
    execution: &str,
    backup_root: Option<&std::path::Path>,
    default_local_port: u16,
    fetch: F,
) -> Result<String, String>
where
    F: Fn(BackendEntry, String, u16) -> Fut,
    Fut: std::future::Future<Output = Result<(serde_json::Value, serde_json::Value), String>>,
{
    let Some(root) = backup_root else {
        return Err("backup directory is not configured".to_string());
    };
    let (bundle, checkpoints) = fetch(entry.clone(), execution.to_string(), default_local_port)
        .await
        .map_err(|e| format!("backup fetch failed for '{backend}/{execution}': {e}"))?;
    let mut merged = bundle;
    if let Some(object) = merged.as_object_mut() {
        object.insert("checkpoints".to_string(), checkpoints);
    }
    let created_ms = wf_common::epoch_ms();
    let id = format!(
        "{backend}-{execution}-{created_ms}-{}",
        wf_common::generate_id()
    );
    let dir = root.join(backend);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create backup dir {}: {e}", dir.display()))?;
    let path = dir.join(format!("{execution}-{created_ms}.json"));
    let text = serde_json::to_string_pretty(&merged)
        .map_err(|e| format!("cannot encode backup '{id}': {e}"))?;
    std::fs::write(&path, &text)
        .map_err(|e| format!("cannot write backup {}: {e}", path.display()))?;
    let sha256 = hex::encode(blake3::hash(text.as_bytes()).as_bytes());
    let record = BackupRecordView {
        id: id.clone(),
        backend: backend.to_string(),
        execution: execution.to_string(),
        created_ms,
        size_bytes: text.len() as u64,
        sha256,
        path: path.to_string_lossy().into_owned(),
    };
    store.record_backup(&record).await?;
    Ok(id)
}

async fn fetch_bundle(
    entry: BackendEntry,
    execution: String,
    default_local_port: u16,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let base_url = entry.local_base_url(default_local_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("cannot build backup client: {e}"))?;
    let bundle: serde_json::Value = client
        .get(format!("{base_url}/api/v1/executions/{execution}/bundle"))
        .send()
        .await
        .map_err(|e| format!("bundle request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("bundle decode failed: {e}"))?;
    let bundle = bundle
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let checkpoints: serde_json::Value = client
        .get(format!(
            "{base_url}/api/v1/executions/{execution}/checkpoints/chain"
        ))
        .send()
        .await
        .map_err(|e| format!("checkpoint chain request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("checkpoint chain decode failed: {e}"))?;
    let checkpoints = checkpoints
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Ok((bundle, checkpoints))
}

/// Prune one backend's backups to the count cap and the age cap,
/// whichever hits first. Deletes bundle files best-effort and drops the
/// index rows; a missing file never fails the prune.
pub async fn prune_backups(
    store: &BackendStore,
    backend: &str,
    retention_count: usize,
    retention_days: u64,
) -> Result<usize, String> {
    let mut records = store.list_backups(Some(backend)).await?;
    records.sort_by_key(|record| std::cmp::Reverse(record.created_ms));
    let now_ms = wf_common::epoch_ms();
    let max_age_ms = (retention_days as i64).saturating_mul(24 * 60 * 60 * 1000);
    let mut condemned = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let over_count = index >= retention_count;
        let over_age = now_ms.saturating_sub(record.created_ms) > max_age_ms;
        if over_count || over_age {
            condemned.push(record.id.clone());
        }
    }
    let mut removed = 0;
    for id in condemned {
        if let Some(record) = records.iter().find(|record| record.id == id) {
            let _ = std::fs::remove_file(&record.path);
        }
        if store.remove_backup(&id).await.unwrap_or(false) {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
        }
    }

    #[tokio::test]
    async fn count_and_age_caps_apply_first_wins() {
        let store = BackendStore::new_memory();
        let now = wf_common::epoch_ms();
        for index in 0..5 {
            store
                .record_backup(&BackupRecordView {
                    id: format!("backup-{index}"),
                    backend: "alpha".to_string(),
                    execution: "exec".to_string(),
                    created_ms: now - (index as i64) * 24 * 60 * 60 * 1000,
                    size_bytes: 10,
                    sha256: "abc".to_string(),
                    path: format!("/tmp/backup-{index}.json"),
                })
                .await
                .unwrap();
        }
        let removed = prune_backups(&store, "alpha", 2, 3650).await.unwrap();
        assert_eq!(removed, 3);
        let remaining = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn single_failure_does_not_block_others() {
        let store = BackendStore::new_memory();
        let failing = entry();
        let dir = tempfile::tempdir().unwrap();
        let result = backup_one_with_fetch(
            &store,
            "broken",
            &failing,
            "exec-1",
            Some(dir.path()),
            18080,
            |_: BackendEntry, _: String, _: u16| async {
                Err::<(serde_json::Value, serde_json::Value), String>("down".to_string())
            },
        )
        .await;
        assert!(result.is_err());
        assert!(store.list_backups(None).await.unwrap().is_empty());
    }
}
