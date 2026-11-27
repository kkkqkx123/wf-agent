//! Two-node integration tests for the remote backup and recovery chain.
//!
//! Every test starts two real servers on loopback ports: an execution node
//! holding the data and a management node with the backend domain enabled.
//! The whole chain runs over real HTTP the way production does it: inventory
//! polling, bundle export, restore, revision rebuild and sync-state
//! bookkeeping. Only the fixture data is written directly, because seeding an
//! execution normally requires a full engine run.

#![cfg(feature = "backends")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;
use wf_api::ApiContext;
use wf_metrics::MetricsRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_server::backends::BackendConfig;
use wf_server::directory::{DirectoryConfig, DirectoryContext};
use wf_server::{ServerConfig, ServerHandle};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::agent_execution::IterationRecord;
use wf_types::execution::ExecutionType;
use wf_types::{AgentExecution, ExecutionStatus, Id};

const BACKEND: &str = "alpha";
const EXECUTION: &str = "exec-backup";
const POLL_LIMIT_SECS: u64 = 20;

/// Enable the restore route once per test binary. The route reads the flag
/// while the router is built, so it must be set before either node starts.
fn enable_restore_route() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("WF_RECOVERY_RESTORE_ENABLED", "1");
    });
}

fn execution_record(id: &str, directory: &str, status: ExecutionStatus) -> AgentExecution {
    AgentExecution {
        id: Id::from(id.to_string()),
        definition_id: Id::from("agent-1".to_string()),
        execution_type: ExecutionType::Agent,
        status,
        current_iteration: 1,
        tool_call_count: 0,
        iteration_history: Some(vec![IterationRecord {
            iteration: 1,
            started_at: 10,
            completed_at: Some(20),
            tool_calls: None,
            response_content: None,
            llm_calls: None,
            error: None,
        }]),
        started_at: 1,
        completed_at: Some(30),
        error: None,
        context: None,
        loop_config: None,
        permanently_failed_tools: None,
        hierarchy: None,
        rerun_of: None,
        directory: Some(directory.to_string()),
    }
}

fn memory_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

fn dirs() -> Arc<DirectoryContext> {
    Arc::new(DirectoryContext::new(DirectoryConfig::default()))
}

async fn serve(ctx: Arc<ApiContext>, config: &ServerConfig) -> ServerHandle {
    wf_server::serve_full_with_config(Arc::new(MetricsRegistry::new()), ctx, config, dirs())
        .await
        .expect("server should bind a loopback port")
}

/// Two running nodes plus the fixture directory both of them agree on.
struct Nodes {
    _tmp: TempDir,
    work: String,
    backup_dir: PathBuf,
    remote_ctx: Arc<ApiContext>,
    remote: ServerHandle,
    admin: ServerHandle,
    client: reqwest::Client,
}

impl Nodes {
    async fn start() -> Self {
        enable_restore_route();
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("work");
        std::fs::create_dir_all(&work_dir).expect("work dir");
        let work = std::fs::canonicalize(&work_dir)
            .expect("canonical work dir")
            .to_string_lossy()
            .into_owned();
        let backup_dir = tmp.path().join("backups");
        std::fs::create_dir_all(&backup_dir).expect("backup dir");

        let remote_ctx = memory_ctx();
        remote_ctx
            .storage
            .agent_execution
            .save(&execution_record(
                EXECUTION,
                &work,
                ExecutionStatus::Completed,
            ))
            .await
            .expect("seed execution on the remote node");

        let remote = serve(
            Arc::clone(&remote_ctx),
            &ServerConfig {
                bind_addr: "127.0.0.1:0".parse().expect("addr"),
                ..ServerConfig::default()
            },
        )
        .await;
        let remote_port = remote.addr().port();

        let admin = serve(
            memory_ctx(),
            &ServerConfig {
                bind_addr: "127.0.0.1:0".parse().expect("addr"),
                backends: BackendConfig {
                    enabled: true,
                    backup_dir: Some(backup_dir.clone()),
                    // Fast loops keep the test wall-clock short; the periodic
                    // full re-check stays off so a deliberate damage stays
                    // visible until the test repairs it.
                    backup_interval_secs: 1,
                    probe_interval_secs: 1,
                    backup_reconcile_interval_secs: 0,
                    ..BackendConfig::default()
                },
                ..ServerConfig::default()
            },
        )
        .await;

        let nodes = Self {
            _tmp: tmp,
            work,
            backup_dir,
            remote_ctx,
            remote,
            admin,
            client: reqwest::Client::new(),
        };
        nodes.register_backend(remote_port).await;
        nodes
    }

    fn admin_url(&self, path: &str) -> String {
        format!("http://{}{}", self.admin.addr(), path)
    }

    fn remote_url(&self, path: &str) -> String {
        format!("http://{}{}", self.remote.addr(), path)
    }

    /// Register the execution node under the management node's registry. The
    /// tunnel port is the node's real port, so the manager reaches it directly.
    async fn register_backend(&self, remote_port: u16) {
        let body = json!({
            "name": BACKEND,
            "entry": {
                "host": "127.0.0.1",
                "username": "tester",
                "local_port": remote_port,
                "server_port": remote_port,
                "backup_directory": self.work,
                "backup_executions": [EXECUTION],
            }
        });
        let response = self
            .client
            .post(self.admin_url("/api/v1/system/backends"))
            .json(&body)
            .send()
            .await
            .expect("register backend");
        assert!(
            response.status().is_success(),
            "backend registration failed: {}",
            response.text().await.unwrap_or_default()
        );
    }

    async fn envelope(&self, response: reqwest::Response) -> (reqwest::StatusCode, Value) {
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn post(&self, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
        let response = self
            .client
            .post(self.admin_url(path))
            .json(&body)
            .send()
            .await
            .expect("admin request");
        self.envelope(response).await
    }

    async fn get(&self, path: &str) -> (reqwest::StatusCode, Value) {
        let response = self
            .client
            .get(self.admin_url(path))
            .send()
            .await
            .expect("admin request");
        self.envelope(response).await
    }

    /// Trigger one snapshot without waiting for the scheduled round.
    async fn backup_now(&self) -> String {
        let (status, body) = self
            .post(
                &format!("/api/v1/system/backends/{BACKEND}/backups"),
                json!({ "execution": EXECUTION }),
            )
            .await;
        assert!(status.is_success(), "backup trigger failed: {body}");
        body["data"]["id"]
            .as_str()
            .expect("backup id in trigger response")
            .to_string()
    }

    async fn backups(&self) -> Vec<Value> {
        let (status, body) = self
            .get(&format!("/api/v1/system/backups?backend={BACKEND}"))
            .await;
        assert!(status.is_success(), "backup list failed: {body}");
        body["data"]["backups"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Current sync state of the target, or `Null` before the first write.
    async fn sync_state(&self) -> Value {
        let (status, body) = self.get("/api/v1/system/backups/sync-state").await;
        assert!(status.is_success(), "sync state failed: {body}");
        body["data"]["states"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["execution"] == EXECUTION && row["backend"] == BACKEND)
                    .cloned()
            })
            .unwrap_or(Value::Null)
    }

    /// Poll the durable state until `predicate` accepts it.
    async fn wait_state(&self, label: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(POLL_LIMIT_SECS);
        let mut last = Value::Null;
        while Instant::now() < deadline {
            last = self.sync_state().await;
            if predicate(&last) {
                return last;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        panic!("timed out waiting for {label}; last state: {last}");
    }

    async fn wait_status(&self, status: &str) -> Value {
        self.wait_state(status, |state| state["repair_status"] == status)
            .await
    }

    /// Apply or preview one snapshot on the remote node.
    async fn restore(
        &self,
        id: &str,
        dry_run: bool,
        replace: bool,
    ) -> (reqwest::StatusCode, Value) {
        self.post(
            &format!("/api/v1/system/backups/{id}/restore"),
            json!({ "dry_run": dry_run, "replace": replace }),
        )
        .await
    }

    /// Digest the remote node currently exports for the target.
    async fn remote_digest(&self) -> Option<String> {
        let response = self
            .client
            .get(self.remote_url(&format!(
                "/api/v1/remote-sync/executions/{EXECUTION}/bundle"
            )))
            .header("x-wf-directory", &self.work)
            .send()
            .await
            .expect("remote bundle request");
        if !response.status().is_success() {
            return None;
        }
        let body: Value = response.json().await.ok()?;
        body["data"]["payload_digest"].as_str().map(str::to_string)
    }

    /// Rebuild revision rows on the remote node from a full export.
    async fn rebuild_remote(&self, minimum_sequence: u64) -> u64 {
        let response = self
            .client
            .post(self.remote_url("/api/v1/remote-sync/revisions/rebuild"))
            .header("x-wf-directory", &self.work)
            .json(&json!({
                "targets": [
                    { "execution_id": EXECUTION, "minimum_sequence": minimum_sequence }
                ]
            }))
            .send()
            .await
            .expect("rebuild request");
        let body: Value = response.json().await.expect("rebuild body");
        body["data"]["executions"][0]["sequence"]
            .as_u64()
            .unwrap_or_else(|| panic!("rebuild produced no sequence: {body}"))
    }
}

#[tokio::test]
async fn missing_execution_is_restored_and_reported_healthy() {
    let nodes = Nodes::start().await;
    let backup = nodes.backup_now().await;
    let healthy = nodes.wait_status("ok").await;
    let digest = healthy["digest"]
        .as_str()
        .expect("baseline digest")
        .to_string();

    // The whole execution disappears from the node: record and ledger row.
    nodes
        .remote_ctx
        .storage
        .agent_execution
        .delete(EXECUTION)
        .await
        .expect("delete remote record");
    nodes
        .remote_ctx
        .storage
        .revisions
        .remove(EXECUTION)
        .await
        .expect("delete remote revision row");

    let lost = nodes.wait_status("remote_missing").await;
    assert!(
        lost["failures"].as_u64().unwrap_or(0) >= 1,
        "a detected loss counts a failure: {lost}"
    );
    assert_eq!(nodes.remote_digest().await, None);

    let (status, body) = nodes.restore(&backup, false, false).await;
    assert!(status.is_success(), "restore failed: {body}");
    let plan = &body["data"];
    assert_eq!(plan["action"], "create");
    assert_eq!(plan["target_state"], "absent");
    assert!(
        plan["remote_sequence"].as_u64().unwrap_or(0) > 0,
        "applied restore reports the written sequence: {plan}"
    );
    assert_eq!(plan["remote_digest"], digest);

    // The content really came back, not just the state row.
    assert_eq!(nodes.remote_digest().await, Some(digest.clone()));

    // The position written by the restore keeps the next round healthy. This
    // is the regression the recovery path used to fail: sequence advanced
    // with an unchanged digest reads as in-place corruption.
    let settled = nodes.wait_status("ok").await;
    assert_eq!(settled["digest"], digest);
    assert_eq!(settled["failures"], 0);
}

#[tokio::test]
async fn identical_target_restore_is_a_noop() {
    let nodes = Nodes::start().await;
    let backup = nodes.backup_now().await;
    let before = nodes.wait_status("ok").await;
    let sequence = before["revision"].as_u64().expect("baseline sequence");

    let (status, body) = nodes.restore(&backup, true, false).await;
    assert!(status.is_success(), "dry run failed: {body}");
    assert_eq!(body["data"]["action"], "noop");
    assert_eq!(body["data"]["target_state"], "identical");

    let (status, body) = nodes.restore(&backup, false, false).await;
    assert!(status.is_success(), "apply failed: {body}");
    assert_eq!(body["data"]["action"], "noop");
    assert_eq!(body["data"]["previous_sequence"], sequence);

    let after = nodes.sync_state().await;
    assert_eq!(
        after["revision"], sequence,
        "a noop must not move the position"
    );
    assert_eq!(after["repair_status"], "ok");
}

#[tokio::test]
async fn divergent_target_needs_replace_and_replace_converges() {
    let nodes = Nodes::start().await;
    let backup = nodes.backup_now().await;
    let snapshot_digest = nodes.wait_status("ok").await["digest"]
        .as_str()
        .expect("snapshot digest")
        .to_string();

    // The remote moved on: the record changed and the ledger advanced, so the
    // snapshot no longer describes the target even though nothing is damaged.
    nodes
        .remote_ctx
        .storage
        .agent_execution
        .save(&execution_record(
            EXECUTION,
            &nodes.work,
            ExecutionStatus::Failed,
        ))
        .await
        .expect("advance remote record");
    let ahead = nodes
        .wait_state("an advanced remote sequence", |state| {
            state["digest"] != snapshot_digest
        })
        .await;
    let ahead_digest = ahead["digest"].as_str().unwrap_or_default().to_string();
    assert_ne!(ahead_digest, snapshot_digest);

    // Without the explicit replace flag nothing is overwritten.
    let (status, body) = nodes.restore(&backup, false, false).await;
    assert!(
        !status.is_success(),
        "a divergent target must be refused without replace: {body}"
    );
    assert_eq!(nodes.remote_digest().await, Some(ahead_digest));

    let (status, body) = nodes.restore(&backup, false, true).await;
    assert!(status.is_success(), "replace failed: {body}");
    assert_eq!(body["data"]["action"], "replace");
    assert_eq!(body["data"]["target_state"], "divergent");
    assert_eq!(body["data"]["remote_digest"], snapshot_digest);

    assert_eq!(nodes.remote_digest().await, Some(snapshot_digest.clone()));

    // The status was already healthy before the restore, so waiting for the
    // label alone can observe the pre-restore position. Wait for the digest
    // to converge as well.
    let settled = nodes
        .wait_state("a converged target after replace", |state| {
            state["repair_status"] == "ok"
                && state["digest"].as_str() == Some(snapshot_digest.as_str())
        })
        .await;
    assert_eq!(settled["digest"], snapshot_digest);
}

#[tokio::test]
async fn lost_revision_table_rebuilds_above_the_known_sequence() {
    let nodes = Nodes::start().await;
    nodes.backup_now().await;
    let baseline = nodes.wait_status("ok").await;
    let sequence = baseline["revision"].as_u64().expect("baseline sequence");
    let digest = baseline["digest"].as_str().expect("digest").to_string();

    nodes
        .remote_ctx
        .storage
        .revisions
        .clear()
        .await
        .expect("clear remote ledger");
    nodes.wait_status("needs_rebuild").await;

    let rebuilt = nodes.rebuild_remote(sequence).await;
    assert_eq!(
        rebuilt, sequence,
        "a rebuild must land on the known baseline, not reset to one"
    );

    let settled = nodes
        .wait_state("a healthy target after rebuild", |state| {
            state["repair_status"] == "ok" && state["revision"] == sequence
        })
        .await;
    assert_eq!(settled["digest"], digest);
    assert_eq!(nodes.remote_digest().await, Some(digest));
}

#[tokio::test]
async fn wrong_directory_scope_does_not_touch_the_target() {
    let nodes = Nodes::start().await;
    let backup = nodes.backup_now().await;
    nodes.wait_status("ok").await;

    let other = nodes._tmp.path().join("other");
    std::fs::create_dir_all(&other).expect("other dir");

    // The bundle was stored under its own directory. Restoring it into a
    // different scope must be refused before any write happens.
    let response = nodes
        .client
        .post(nodes.remote_url("/api/v1/remote-sync/executions/restore"))
        .header("x-wf-directory", other.to_string_lossy().into_owned())
        .json(&json!({
            "bundle": nodes.exported_bundle(&backup).await,
            "dry_run": false,
            "mode": "replace",
        }))
        .send()
        .await
        .expect("restore request");
    let status = response.status();
    assert!(
        !status.is_success(),
        "a restore across directories must be refused, got {status}"
    );
    // The originally owned target still exports its own content.
    assert!(nodes.remote_digest().await.is_some());
}

impl Nodes {
    /// Read the stored bundle payload from the management export endpoint.
    async fn exported_bundle(&self, id: &str) -> Value {
        let (status, body) = self
            .get(&format!("/api/v1/system/backups/{id}/export"))
            .await;
        assert!(status.is_success(), "export failed: {body}");
        body["data"]["payload"].clone()
    }
}

#[tokio::test]
async fn damaged_local_snapshot_is_refetched() {
    let nodes = Nodes::start().await;
    let backup = nodes.backup_now().await;
    nodes.wait_status("ok").await;

    let records = nodes.backups().await;
    let record = records
        .iter()
        .find(|row| row["id"] == backup)
        .unwrap_or_else(|| panic!("backup index is missing {backup}: {records:?}"));
    let path = record["path"].as_str().expect("backup file path");
    // The index stores paths relative to the backup root so a moved root
    // keeps working; the test resolves them the same way the server does.
    let file = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        nodes.backup_dir.join(path)
    };
    std::fs::write(&file, b"{}").expect("damage the snapshot");

    // The next round finds the newest snapshot unusable and pulls a fresh one
    // immediately instead of waiting for the interval to elapse.
    let deadline = Instant::now() + Duration::from_secs(POLL_LIMIT_SECS);
    let mut refetched = false;
    while Instant::now() < deadline {
        let records = nodes.backups().await;
        if records.iter().any(|row| row["id"] != backup) {
            refetched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    assert!(refetched, "a damaged local snapshot must be replaced");

    let settled = nodes.wait_status("ok").await;
    assert_eq!(settled["local_status"], "ok");
    let digest = settled["digest"].as_str().unwrap_or_default().to_string();
    assert_eq!(nodes.remote_digest().await, Some(digest));
}
