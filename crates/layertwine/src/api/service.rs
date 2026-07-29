use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::backup::backup_repo::BackupRepo;
use crate::checkpoint::repo::CheckpointRepo;
use crate::checkpoint::types::Checkpoint;
use crate::config::{CompactOptions, LayertwineConfig};
use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::snapshot::Snapshot;
use crate::core::types::LineDiff;
use crate::core::types::{
    AgentInstanceId, CheckpointId, ContentId, DiffOp, PartitionType, SnapshotId, SourceType,
};
use crate::error::{LayertwineError, Result as LayertwineResult};
use crate::git_sync::gc::collect_garbage;
use crate::git_sync::git_bridge::GitBridge;
use crate::layered::StateMachine;
use crate::storage::repository::{
    AtomicOps, CheckpointPersist, DeltaStore, FileNodeStore, LayerStore, PartitionStore,
    SnapshotStore,
};
use crate::storage::SqliteStorage;

use super::types::*;

/// Unified service configuration
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub db_path: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        ServiceConfig {
            db_path: ".layertwine/layertwine.db".into(),
        }
    }
}



// ── Helpers ──

fn open_storage(db_path: &str) -> LayertwineResult<Arc<SqliteStorage>> {
    let path = Path::new(db_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LayertwineError::General(format!("failed to create db directory: {}", e))
        })?;
    }
    let storage = SqliteStorage::new_full(path).map_err(LayertwineError::Storage)?;
    Ok(Arc::new(storage))
}

fn load_checkpoint_repo(storage: &SqliteStorage) -> LayertwineResult<CheckpointRepo> {
    let persist: Box<dyn CheckpointPersist> = Box::new(storage.share());
    CheckpointRepo::load(persist)
}

fn map_error(e: LayertwineError) -> ApiError {
    match e {
        LayertwineError::Storage(se) => ApiError::storage(se.to_string()),
        LayertwineError::Engine(s) => ApiError::engine(s),
        LayertwineError::StateMachine(s) => ApiError::state_machine(s),
        LayertwineError::Checkpoint(s) => ApiError::checkpoint(s),
        LayertwineError::Restore(s) => ApiError::checkpoint(format!("restore: {}", s)),
        LayertwineError::Transaction(s) => ApiError::checkpoint(format!("transaction: {}", s)),
        LayertwineError::Integrity(s) => ApiError::checkpoint(format!("integrity: {}", s)),
        LayertwineError::GitSync(s) => ApiError::git_sync(s),
        LayertwineError::Gc(s) => ApiError::gc(s),
        LayertwineError::NotFound(s) => ApiError::not_found(s),
        LayertwineError::Cli {
            context,
            suggestion,
        } => ApiError {
            code: "CLI_ERROR".into(),
            message: context,
            suggestion,
            details: None,
        },
        LayertwineError::Serialization(s) => ApiError::internal(format!("serialization: {}", s)),
        LayertwineError::General(s) => ApiError::general(s),
    }
}

fn snapshot_id_to_hex(id: &SnapshotId) -> String {
    id.to_hex()
}

fn checkpoint_to_info(cp: &Checkpoint) -> CheckpointInfo {
    CheckpointInfo {
        id: cp.id.to_hex(),
        author: cp.metadata.author.clone(),
        message: cp.metadata.message.clone(),
        parents: cp.parents.iter().map(|p| p.to_hex()).collect(),
        snapshots: cp.baseline_snapshots.iter().map(|s| s.to_hex()).collect(),
        created_at: cp.created_at,
        git_anchor: cp.metadata.git_anchor.clone(),
    }
}

// ── ApiService ──

/// Default implementation of the layertwine service.
///
/// Wraps StateMachine and SqliteStorage, providing a structured API
/// that all transport layers (CLI, HTTP, gRPC) can use.
pub struct ApiService {
    storage: Arc<SqliteStorage>,
    state_machine: StateMachine<SqliteStorage>,
    checkpoint_repo: Arc<std::sync::RwLock<CheckpointRepo>>,
    db_path: String,
    maintenance_cfg: crate::config::MaintenanceConfig,
    backup_repo: Arc<BackupRepo>,
}

impl ApiService {
    /// Open an existing layertwine repository
    pub fn open(config: ServiceConfig) -> ApiResult<Self> {
        let storage = open_storage(&config.db_path).map_err(map_error)?;
        let state_machine = StateMachine::new(storage.clone());

        // Load config via priority chain:
        //   defaults → ~/.config/layertwine.toml → <binary-dir>/layertwine.toml → <db-dir>/layertwine.toml
        let db_dir = Path::new(&config.db_path)
            .parent()
            .unwrap_or(Path::new("."));
        let strat_cfg = LayertwineConfig::load_with_priority(db_dir).unwrap_or_default();
        let maintenance_cfg = strat_cfg.maintenance;

        // Load checkpoint repo
        let persist: Box<dyn CheckpointPersist> = Box::new(storage.share());
        let checkpoint_repo = CheckpointRepo::load(persist).map_err(map_error)?;

        // Open backup repo (dedicated SQLite DB for physical isolation)
        let backup_db_path = db_dir.join("layertwine-backup.db");
        let backup_repo = Arc::new(
            BackupRepo::new(&backup_db_path)
                .map_err(|e| map_error(LayertwineError::Storage(e)))?,
        );

        Ok(ApiService {
            storage,
            state_machine,
            checkpoint_repo: Arc::new(std::sync::RwLock::new(checkpoint_repo)),
            db_path: config.db_path,
            maintenance_cfg,
            backup_repo,
        })
    }

    /// Reconstruct text from a snapshot by its ID
    fn reconstruct_text_from_id(&self, snapshot_id: &SnapshotId) -> ApiResult<String> {
        let snapshot = self
            .storage
            .get_snapshot(snapshot_id)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        crate::layered::transition::reconstruct_text(self.storage.as_ref(), &snapshot)
            .map_err(map_error)
    }

    /// Get the "before" text from the last checkpoint (or empty string if none)
    fn last_checkpoint_text(&self) -> String {
        match self.storage.list_checkpoints() {
            Ok(cps) if !cps.is_empty() => {
                let cp = &cps[0];
                cp.baseline_snapshots
                    .first()
                    .and_then(|sid| self.reconstruct_text_from_id(sid).ok())
                    .unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    /// Compute diff stats from a LineDiff
    fn diff_stats(diff: &LineDiff) -> (usize, usize) {
        let mut inserts = 0usize;
        let mut deletes = 0usize;
        for hunk in &diff.hunks {
            for op in &hunk.ops {
                match op {
                    DiffOp::Insert { lines, .. } => inserts += lines.len(),
                    DiffOp::Delete { count, .. } => deletes += *count as usize,
                    DiffOp::Replace {
                        old_count, lines, ..
                    } => {
                        deletes += *old_count as usize;
                        inserts += lines.len();
                    }
                    DiffOp::Equal { .. } => {}
                }
            }
        }
        (inserts, deletes)
    }

    /// Show staged changes vs last committed checkpoint
    fn show_staged(&self) -> ApiResult<ShowResponse> {
        let staged_pid = crate::layered::staged::staged_partition_id();
        let staged_partition = self
            .storage
            .get_partition(&staged_pid)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let staged_snapshot = self
            .storage
            .get_snapshot(&staged_partition.current_snapshot)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;

        let new_text = self.reconstruct_text_from_id(&staged_partition.current_snapshot)?;
        let old_text = self.last_checkpoint_text();
        let staged_deltas = self
            .storage
            .get_deltas(&staged_snapshot.deltas)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let file_path = staged_deltas
            .last()
            .map(|d| d.file.path_str().to_string())
            .unwrap_or_else(|| staged_snapshot.file.path_str().to_string());

        let unified_diff = crate::engine::diff::format_unified_diff(&old_text, &new_text, 3);
        let line_diff = crate::engine::diff::diff_to_line_diff(&old_text, &new_text);
        let (inserts, deletes) = Self::diff_stats(&line_diff);

        Ok(ShowResponse {
            target: "staged".into(),
            diffs: vec![FileDiff {
                file_path,
                unified_diff,
                inserts,
                deletes,
            }],
        })
    }

    /// Show diff for a checkpoint vs its parent
    fn show_checkpoint(&self, id_str: &str) -> ApiResult<ShowResponse> {
        let cp_id = ContentId::from_hex(id_str).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid checkpoint ID '{}'", id_str))
        })?;
        let checkpoint = self
            .storage
            .get_checkpoint(&cp_id)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;

        // "After" text from the checkpoint's baseline snapshot
        let new_snapshot_id = checkpoint
            .baseline_snapshots
            .first()
            .ok_or_else(|| ApiError::internal("checkpoint has no baseline snapshots"))?;
        let new_snapshot = self
            .storage
            .get_snapshot(new_snapshot_id)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let new_text = self.reconstruct_text_from_id(new_snapshot_id)?;
        let new_deltas = self
            .storage
            .get_deltas(&new_snapshot.deltas)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let file_path = new_deltas
            .last()
            .map(|d| d.file.path_str().to_string())
            .unwrap_or_else(|| new_snapshot.file.path_str().to_string());

        // "Before" text from the parent checkpoint's baseline snapshot
        let old_text = match checkpoint.parents.first() {
            Some(parent_id) => match self.storage.get_checkpoint(parent_id) {
                Ok(parent_cp) => parent_cp
                    .baseline_snapshots
                    .first()
                    .and_then(|sid| self.reconstruct_text_from_id(sid).ok())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            },
            None => String::new(),
        };

        let unified_diff = crate::engine::diff::format_unified_diff(&old_text, &new_text, 3);
        let line_diff = crate::engine::diff::diff_to_line_diff(&old_text, &new_text);
        let (inserts, deletes) = Self::diff_stats(&line_diff);

        Ok(ShowResponse {
            target: format!("checkpoint:{}", id_str),
            diffs: vec![FileDiff {
                file_path,
                unified_diff,
                inserts,
                deletes,
            }],
        })
    }

    /// Show diff for a partition vs last checkpoint
    fn show_partition(&self, name: &str) -> ApiResult<ShowResponse> {
        let partition = self
            .storage
            .get_partition_by_name(name)
            .map_err(|_| ApiError::not_found(format!("partition '{}'", name)))?;

        let snapshot = self
            .storage
            .get_snapshot(&partition.current_snapshot)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let new_text = self.reconstruct_text_from_id(&partition.current_snapshot)?;
        let partition_deltas = self
            .storage
            .get_deltas(&snapshot.deltas)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let file_path = partition_deltas
            .last()
            .map(|d| d.file.path_str().to_string())
            .unwrap_or_else(|| snapshot.file.path_str().to_string());
        let old_text = self.last_checkpoint_text();

        let unified_diff = crate::engine::diff::format_unified_diff(&old_text, &new_text, 3);
        let line_diff = crate::engine::diff::diff_to_line_diff(&old_text, &new_text);
        let (inserts, deletes) = Self::diff_stats(&line_diff);

        Ok(ShowResponse {
            target: format!("partition:{}", name),
            diffs: vec![FileDiff {
                file_path,
                unified_diff,
                inserts,
                deletes,
            }],
        })
    }

    pub fn init(&self, req: InitRequest) -> ApiResult<InitResponse> {
        let db_path = req.db_path.clone().unwrap_or_else(|| self.db_path.clone());
        let storage = open_storage(&db_path).map_err(map_error)?;

        if let Some(git_repo_path) = &req.git_repo {
            let git_path = Path::new(git_repo_path);
            let ref_name = req.git_ref.as_deref().unwrap_or("HEAD");
            let persist: Box<dyn CheckpointPersist> = Box::new(storage.share());
            let mut checkpoint_repo = CheckpointRepo::load(persist).map_err(map_error)?;

            GitBridge::init_from_git(git_path, &*storage, &mut checkpoint_repo, ref_name)
                .map_err(map_error)?;

            // Auto-persist any metadata changes made inside init_from_git (e.g. git_anchor)
            checkpoint_repo.sync_all().map_err(map_error)?;

            Ok(InitResponse {
                db_path: db_path.clone(),
                manual_partition_id: String::new(),
                staged_partition_id: String::new(),
                branch: "main".into(),
            })
        } else {
            let file_node = FileNode::new(PathBuf::from(".layertwine/init"), b"");
            storage
                .store_file_node(&file_node, b"")
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;
            let empty_diff = Delta::new(
                file_node.clone(),
                crate::core::types::LineDiff::new(vec![]),
                SourceType::Manual,
            );
            storage
                .store_delta(&empty_diff)
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;
            let initial_snapshot = Snapshot::new_initial(file_node, empty_diff.id);
            storage
                .store_snapshot(&initial_snapshot, b"")
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;

            let manual_partition = crate::layered::manual::ensure_manual_partition(
                storage.as_ref(),
                initial_snapshot.id,
            )
            .map_err(map_error)?;
            let staged_partition = crate::layered::staged::ensure_staged_partition(
                storage.as_ref(),
                initial_snapshot.id,
            )
            .map_err(map_error)?;

            let persist: Box<dyn CheckpointPersist> = Box::new(storage.share());
            let mut _checkpoint_repo = CheckpointRepo::load(persist).map_err(map_error)?;

            Ok(InitResponse {
                db_path: db_path.clone(),
                manual_partition_id: manual_partition.id.to_string(),
                staged_partition_id: staged_partition.id.to_string(),
                branch: "main".into(),
            })
        }
    }

    pub fn status(&self) -> ApiResult<StatusResponse> {
        let partitions = self
            .storage
            .list_partitions()
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let infos = partitions
            .iter()
            .map(|p| {
                let layer = match &p.partition_type {
                    PartitionType::Manual => "manual_edit",
                    PartitionType::Agent(_) => "agent_edit",
                    PartitionType::Approval(_) => "approval",
                    PartitionType::Integrated(_) => "integrated",
                    PartitionType::Unified => "unified",
                    PartitionType::Staged => "staged",
                };
                PartitionInfo {
                    layer: layer.into(),
                    name: p.name.clone(),
                    current_snapshot: p.current_snapshot.to_hex(),
                    history_len: p.history.len(),
                }
            })
            .collect();
        Ok(StatusResponse { partitions: infos })
    }

    pub fn edit(&self, req: EditRequest) -> ApiResult<EditResponse> {
        let content = req.content.as_deref().ok_or_else(|| {
            ApiError::invalid_params(
                "edit content is required (provide via -c/--content or pipe via stdin)"
            )
        })?;
        let snapshot_id =
            crate::layered::manual::apply_manual_edit(self.storage.as_ref(), &req.file, content)
                .map_err(map_error)?;

        let staged_snapshot_id =
            crate::layered::manual::merge_manual_to_staged(self.storage.as_ref())
                .map_err(map_error)
                .ok();

        Ok(EditResponse {
            snapshot_id: snapshot_id_to_hex(&snapshot_id),
            staged_snapshot_id: staged_snapshot_id.map(|id| snapshot_id_to_hex(&id)),
        })
    }

    pub fn agent_edit(&self, req: AgentEditRequest) -> ApiResult<EditResponse> {
        let agent_instance = AgentInstanceId(req.agent_id.clone());
        let content = req.content.as_deref().ok_or_else(|| {
            ApiError::invalid_params(
                "edit content is required (provide via -c/--content or pipe via stdin)",
            )
        })?;

        let staged_pid = crate::layered::staged::staged_partition_id();
        let initial_snapshot = match self.storage.get_partition(&staged_pid) {
            Ok(p) => p.current_snapshot,
            Err(_) => {
                let file_node = FileNode::new(PathBuf::from(&req.file), content.as_bytes());
                self.storage
                    .store_file_node(&file_node, content.as_bytes())
                    .map_err(|e| map_error(LayertwineError::Storage(e)))?;
                let delta = Delta::new(
                    file_node,
                    crate::core::types::LineDiff::new(vec![]),
                    SourceType::Agent(agent_instance.clone()),
                );
                self.storage
                    .store_delta(&delta)
                    .map_err(|e| map_error(LayertwineError::Storage(e)))?;
                let snapshot = Snapshot::new_initial(
                    FileNode::new(PathBuf::from(&req.file), content.as_bytes()),
                    delta.id,
                );
                self.storage
                    .store_snapshot(&snapshot, content.as_bytes())
                    .map_err(|e| map_error(LayertwineError::Storage(e)))?;
                snapshot.id
            }
        };

        let _ = crate::layered::agent::ensure_agent_partition(
            self.storage.as_ref(),
            &agent_instance,
            initial_snapshot,
        )
        .map_err(map_error)?;

        let snapshot_id = crate::layered::agent::apply_agent_edit(
            self.storage.as_ref(),
            &agent_instance,
            &req.file,
            content,
        )
        .map_err(map_error)?;

        Ok(EditResponse {
            snapshot_id: snapshot_id_to_hex(&snapshot_id),
            staged_snapshot_id: None,
        })
    }

    pub fn agent_submit(&self, req: AgentSubmitRequest) -> ApiResult<SubmitResponse> {
        let agent_instance = AgentInstanceId(req.agent_id.clone());

        let staged_pid = crate::layered::staged::staged_partition_id();
        let base_snapshot = self
            .storage
            .get_partition(&staged_pid)
            .map_err(|_| ApiError::invalid_params("no staged partition found. Make edits first."))?
            .current_snapshot;

        let _ = crate::layered::approval::ensure_approval_agent_partition(
            self.storage.as_ref(),
            &agent_instance,
            base_snapshot,
        )
        .map_err(map_error)?;

        let snapshot_id =
            crate::layered::agent::move_agent_to_approval(self.storage.as_ref(), &agent_instance)
                .map_err(|e| {
                // Check if the error is because agent partition doesn't exist
                let agent_pid = crate::layered::agent::agent_partition_id(&agent_instance);
                if self.storage.get_partition(&agent_pid).is_err() {
                    ApiError::invalid_params(format!(
                        "agent '{}' has not made any edits yet. Call agent_edit first.",
                        req.agent_id
                    ))
                } else {
                    map_error(e)
                }
            })?;

        Ok(SubmitResponse {
            snapshot_id: snapshot_id_to_hex(&snapshot_id),
        })
    }

    /// Convenience wrapper: approve agent, then merge all integrated → staged.
    /// Uses `approve_agent` and then merges features directly to staged.
    pub fn approve(&self, req: ApproveRequest) -> ApiResult<ApproveResponse> {
        let approve_resp = self.approve_agent(ApproveAgentRequest {
            agent_id: req.agent_id.clone(),
            integrated_name: Some(req.agent_id.clone()),
        })?;

        // Auto-detect all integrated partitions and merge directly to staged
        let names = self
            .storage
            .list_partitions()
            .ok()
            .map(|partitions| {
                partitions
                    .into_iter()
                    .filter_map(|p| match p.partition_type {
                        crate::core::types::PartitionType::Integrated(name) => Some(name),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let staged_snapshot_id = if names.is_empty() {
            approve_resp.integrated_snapshot_id.clone()
        } else {
            let result = crate::layered::staged::merge_features_to_staged(
                self.storage.as_ref(),
                &names,
            )
            .map_err(map_error)?;
            snapshot_id_to_hex(&result.snapshot_id)
        };

        Ok(ApproveResponse {
            integrated_snapshot_id: approve_resp.integrated_snapshot_id,
            staged_snapshot_id,
        })
    }

    pub fn commit(&self, req: CommitRequest) -> ApiResult<CommitResponse> {
        let author = req.author.as_deref().unwrap_or("user");

        // Get staged partition
        let staged_pid = crate::layered::staged::staged_partition_id();
        let staged_partition = self
            .storage
            .get_partition(&staged_pid)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let current_snapshot_id = staged_partition.current_snapshot;

        // Commit using checkpoint repo
        let mut checkpoint_repo = self
            .checkpoint_repo
            .write()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;
        let cp_id = checkpoint_repo
            .commit_single(current_snapshot_id, &req.message, author)
            .map_err(map_error)?;

        // Propagate snapshot source from storage to the checkpoint
        if let Ok(snapshot) = self.storage.get_snapshot(&current_snapshot_id) {
            let source = if !snapshot.source.is_empty() {
                snapshot.source.clone()
            } else {
                format!("file://{}", snapshot.file.path_str())
            };
            let _ = checkpoint_repo.set_snapshot_source(&cp_id, current_snapshot_id, source);
            let _ = checkpoint_repo.sync_checkpoint(&cp_id);
        }

        Ok(CommitResponse {
            checkpoint_id: cp_id.to_hex(),
            message: req.message.clone(),
        })
    }

    pub fn log(&self, req: LogRequest) -> ApiResult<LogResponse> {
        let count = req.count.unwrap_or(20);

        // Use checkpoint repo to get log for current branch
        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;
        let checkpoints = checkpoint_repo.log(count);
        let total = checkpoints.len();

        Ok(LogResponse {
            checkpoints: checkpoints.into_iter().map(checkpoint_to_info).collect(),
            total,
        })
    }

    pub fn branch_create(&self, req: BranchCreateRequest) -> ApiResult<BranchCreateResponse> {
        let mut checkpoint_repo = self
            .checkpoint_repo
            .write()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        if checkpoint_repo.branches.iter().any(|b| b.name == req.name) {
            return Err(ApiError::invalid_params(format!(
                "branch '{}' already exists",
                req.name
            )));
        }

        // Create branch from current branch head
        checkpoint_repo
            .create_branch(&req.name)
            .map_err(map_error)?;

        let branch_idx = checkpoint_repo.find_branch(&req.name).map_err(map_error)?;
        let head = checkpoint_repo.branches[branch_idx].head;

        drop(checkpoint_repo);

        Ok(BranchCreateResponse {
            name: req.name,
            head: head.to_hex(),
        })
    }

    pub fn branch_switch(&self, req: BranchSwitchRequest) -> ApiResult<BranchSwitchResponse> {
        let _ = self
            .storage
            .get_branch(&req.name)
            .map_err(|_| ApiError::not_found(format!("branch '{}'", req.name)))?;

        // Update checkpoint repo's current branch
        let mut checkpoint_repo = self
            .checkpoint_repo
            .write()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;
        let cp_id = checkpoint_repo
            .switch_branch(&req.name)
            .map_err(map_error)?;

        // Reset staged partition to the branch's base snapshot
        let cp_id2 = self
            .state_machine
            .switch_branch(&req.name)
            .map_err(map_error)?;

        drop(checkpoint_repo);

        assert_eq!(cp_id, cp_id2, "Checkpoint IDs should match");

        Ok(BranchSwitchResponse {
            name: req.name,
            checkpoint_id: cp_id.to_hex(),
        })
    }

    pub fn branch_list(&self) -> ApiResult<BranchListResponse> {
        let branches = self
            .storage
            .list_branches()
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;
        let current_name = checkpoint_repo.current_branch_name().to_string();
        let infos = branches
            .iter()
            .map(|b| BranchInfo {
                name: b.name.clone(),
                head: b.head.to_hex(),
                updated_at: b.updated_at.to_string(),
                is_current: b.name == current_name,
            })
            .collect();
        Ok(BranchListResponse {
            branches: infos,
            current: Some(current_name),
        })
    }

    pub fn merge(&self, req: MergeRequest) -> ApiResult<MergeResponse> {
        let mut repo = load_checkpoint_repo(self.storage.as_ref()).map_err(map_error)?;
        let current_name = repo.current_branch_name().to_string();

        // Get source branch's staged snapshots
        let source_head = repo.get_branch_head(&req.branch).map_err(map_error)?;
        let source_checkpoint = repo
            .checkpoints
            .get(&source_head)
            .ok_or_else(|| ApiError::not_found("source checkpoint not found".to_string()))?;

        // Use the baseline_snapshots from source branch's head checkpoint
        let snapshot_ids = source_checkpoint.baseline_snapshots.clone();

        // Update current staged partition with source snapshots
        let staged_pid = crate::layered::staged::staged_partition_id();
        for snapshot_id in &snapshot_ids {
            self.storage
                .update_pointer(&staged_pid, snapshot_id)
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        }

        let msg = req.message.clone().unwrap_or_else(|| "merge".into());
        let cp_id = repo
            .merge_branches(&req.branch, snapshot_ids, &msg, "user")
            .map_err(map_error)?;

        // merge_branches auto-persists with embedded storage
        Ok(MergeResponse {
            checkpoint_id: cp_id.to_hex(),
            source_branch: req.branch,
            target_branch: current_name,
        })
    }

    pub fn backup(&self, req: BackupRequest) -> ApiResult<BackupResponse> {
        let snapshot_id = ContentId::from_hex(&req.snapshot_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid snapshot ID '{}'", req.snapshot_id))
        })?;

        let backup_id = self
            .backup_repo
            .backup_snapshot(self.storage.as_ref(), snapshot_id, req.label.clone())
            .map_err(map_error)?;

        Ok(BackupResponse {
            backup_id: backup_id.to_hex(),
            source_snapshot_id: req.snapshot_id,
            label: req.label,
        })
    }

    pub fn restore(&self, req: RestoreRequest) -> ApiResult<RestoreResponse> {
        let backup_id = ContentId::from_hex(&req.backup_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid backup ID '{}'", req.backup_id))
        })?;

        let backup = self.backup_repo.get_backup(&backup_id).map_err(map_error)?;

        // Write back the file content to core storage so the base is available for reconstruction
        self.storage
            .store_file_node(&backup.file, &backup.file_content)
            .map_err(|e| map_error(LayertwineError::Storage(e)))?;

        // Store deltas back to core storage
        let delta_count = backup.deltas.len();
        for delta in &backup.deltas {
            self.storage
                .store_delta(delta)
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        }

        // Perform 3-way merge of backup into staged partition
        let merged_id = self
            .backup_repo
            .merge_to_staged(&backup_id, self.storage.as_ref())
            .map_err(map_error)?;

        let restore_file = backup
            .deltas
            .last()
            .map(|d| d.file.path_str().to_string())
            .unwrap_or_else(|| backup.file.path_str().to_string());

        Ok(RestoreResponse {
            backup_id: req.backup_id,
            file: restore_file,
            deltas_restored: delta_count,
            merged_snapshot_id: merged_id.to_hex(),
        })
    }

    // ── Checkpoint restore implementations ──

    pub fn checkpoint_restore(
        &self,
        req: CheckpointRestoreRequest,
    ) -> ApiResult<CheckpointRestoreResponse> {
        let cp_id = CheckpointId::from_hex(&req.checkpoint_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid checkpoint ID '{}'", req.checkpoint_id))
        })?;

        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        let chk_req = crate::checkpoint::restore::RestoreRequest {
            checkpoint_id: Some(cp_id),
            source_filter: req.source_filter.clone(),
            time_range: None,
        };

        let resp = checkpoint_repo.restore(&chk_req).map_err(map_error)?;

        // Translate Checkpoint → CheckpointInfo (the original is fine for CheckpointInfo usage,
        // but restore response also includes full snapshot contents)
        let cp_info = CheckpointInfo {
            id: resp.checkpoint.id.to_hex(),
            author: resp.checkpoint.metadata.author.clone(),
            message: resp.checkpoint.metadata.message.clone(),
            parents: resp.checkpoint.parents.iter().map(|p| p.to_hex()).collect(),
            snapshots: resp
                .checkpoint
                .baseline_snapshots
                .iter()
                .map(|s| s.to_hex())
                .collect(),
            created_at: resp.checkpoint.created_at,
            git_anchor: resp.checkpoint.metadata.git_anchor.clone(),
        };

        let snapshots: Vec<RestoredSnapshotInfo> = resp
            .snapshots
            .into_iter()
            .map(|(snap_id, content, source)| {
                let content_type = content.content_type().to_string();
                let content_hex = hex::encode(content.to_bytes());
                let effective_source = if !source.is_empty() {
                    source
                } else {
                    resp.checkpoint
                        .snapshot_sources
                        .get(&snap_id)
                        .cloned()
                        .unwrap_or_default()
                };
                RestoredSnapshotInfo {
                    snapshot_id: snap_id.to_hex(),
                    source: effective_source,
                    content_hex,
                    content_type,
                }
            })
            .collect();

        let ancestry: Vec<String> = resp.ancestry.iter().map(|id| id.to_hex()).collect();

        Ok(CheckpointRestoreResponse {
            checkpoint: cp_info,
            snapshots,
            ancestry,
        })
    }

    pub fn checkpoint_restore_by_time(
        &self,
        req: CheckpointRestoreByTimeRequest,
    ) -> ApiResult<CheckpointRestoreResponse> {
        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        let resp = checkpoint_repo
            .restore_by_time(
                req.target_time,
                req.source_filter
                    .as_ref()
                    .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<&str>>())
                    .as_deref(),
            )
            .map_err(map_error)?;

        let cp_info = CheckpointInfo {
            id: resp.checkpoint.id.to_hex(),
            author: resp.checkpoint.metadata.author.clone(),
            message: resp.checkpoint.metadata.message.clone(),
            parents: resp.checkpoint.parents.iter().map(|p| p.to_hex()).collect(),
            snapshots: resp
                .checkpoint
                .baseline_snapshots
                .iter()
                .map(|s| s.to_hex())
                .collect(),
            created_at: resp.checkpoint.created_at,
            git_anchor: resp.checkpoint.metadata.git_anchor.clone(),
        };

        let snapshots: Vec<RestoredSnapshotInfo> = resp
            .snapshots
            .into_iter()
            .map(|(snap_id, content, source)| {
                let content_type = content.content_type().to_string();
                let content_hex = hex::encode(content.to_bytes());
                let effective_source = if !source.is_empty() {
                    source
                } else {
                    resp.checkpoint
                        .snapshot_sources
                        .get(&snap_id)
                        .cloned()
                        .unwrap_or_default()
                };
                RestoredSnapshotInfo {
                    snapshot_id: snap_id.to_hex(),
                    source: effective_source,
                    content_hex,
                    content_type,
                }
            })
            .collect();

        let ancestry: Vec<String> = resp.ancestry.iter().map(|id| id.to_hex()).collect();

        Ok(CheckpointRestoreResponse {
            checkpoint: cp_info,
            snapshots,
            ancestry,
        })
    }

    pub fn checkpoint_diff(&self, req: CheckpointDiffRequest) -> ApiResult<CheckpointDiffResponse> {
        let from_id = CheckpointId::from_hex(&req.from_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid from_id '{}'", req.from_id))
        })?;
        let to_id = CheckpointId::from_hex(&req.to_id)
            .ok_or_else(|| ApiError::invalid_params(format!("invalid to_id '{}'", req.to_id)))?;

        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        let diff = checkpoint_repo
            .diff_checkpoints(&from_id, &to_id)
            .map_err(map_error)?;

        Ok(CheckpointDiffResponse {
            from_id: diff.from_id.to_hex(),
            to_id: diff.to_id.to_hex(),
            added: diff.added.iter().map(|id| id.to_hex()).collect(),
            removed: diff.removed.iter().map(|id| id.to_hex()).collect(),
            modified: diff.modified.iter().map(|id| id.to_hex()).collect(),
            total_changes: diff.total_changes(),
        })
    }

    pub fn checkpoint_rollback(
        &self,
        req: CheckpointRollbackRequest,
    ) -> ApiResult<CheckpointRollbackResponse> {
        let cp_id = CheckpointId::from_hex(&req.checkpoint_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid checkpoint ID '{}'", req.checkpoint_id))
        })?;

        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        let snapshot_ids = checkpoint_repo.rollback_to(&cp_id).map_err(map_error)?;

        // Update staged partition to point to the first baseline snapshot from the rollback target
        let staged_pid = crate::layered::staged::staged_partition_id();
        if let Some(first_snap) = snapshot_ids.first() {
            self.storage
                .update_pointer(&staged_pid, first_snap)
                .map_err(|e| map_error(LayertwineError::Storage(e)))?;
        }

        let hex_ids: Vec<String> = snapshot_ids.iter().map(|id| id.to_hex()).collect();

        Ok(CheckpointRollbackResponse {
            checkpoint_id: req.checkpoint_id,
            snapshot_ids: hex_ids,
        })
    }

    pub fn checkpoint_restore_and_apply(
        &self,
        req: CheckpointRestoreApplyRequest,
    ) -> ApiResult<CheckpointRestoreApplyResponse> {
        // 1. Parse checkpoint ID
        let cp_id = CheckpointId::from_hex(&req.checkpoint_id).ok_or_else(|| {
            ApiError::invalid_params(format!("invalid checkpoint ID '{}'", req.checkpoint_id))
        })?;

        // 2. Build internal restore request
        let source_filter: Option<Vec<&str>> = req
            .source_filter
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect());

        let chk_req = crate::checkpoint::restore::RestoreRequest {
            checkpoint_id: Some(cp_id),
            source_filter: req.source_filter.clone(),
            time_range: None,
        };

        // 3. Execute restore + apply via core library (keeps lock guard alive)
        let checkpoint_repo = self
            .checkpoint_repo
            .read()
            .map_err(|e| ApiError::internal(format!("Failed to acquire lock: {}", e)))?;

        let resp = checkpoint_repo.restore(&chk_req).map_err(map_error)?;

        // 4. Delegate file writing to core CheckpointRepo::apply_restore
        let apply_result = if !req.skip_write {
            checkpoint_repo
                .apply_restore(&resp, source_filter.as_deref())
                .map_err(map_error)?
        } else {
            crate::checkpoint::restore::RestoreApplyResult {
                checkpoint_id: resp.checkpoint.id,
                files_written: vec![],
            }
        };

        drop(checkpoint_repo);

        // 5. Update staged partition pointer if requested
        if req.update_staged {
            for (snap_id, _content, source) in &resp.snapshots {
                if !source.starts_with("file://") {
                    continue;
                }
                let staged_pid = crate::layered::staged::staged_partition_id();
                let _ = self
                    .storage
                    .update_pointer(&staged_pid, snap_id)
                    .map_err(|e| {
                        ApiError::storage(format!(
                            "failed to update staged partition pointer: {}",
                            e
                        ))
                    });
            }
        }

        Ok(CheckpointRestoreApplyResponse {
            checkpoint_id: req.checkpoint_id,
            files_written: apply_result.files_written,
            staged_updated: req.update_staged,
        })
    }

    pub fn gc(&self, _req: GcRequest) -> ApiResult<GcResponse> {
        let mut repo = load_checkpoint_repo(self.storage.as_ref()).map_err(map_error)?;
        let stats = collect_garbage(&mut repo).map_err(map_error)?;
        // remove_checkpoint auto-persists; sync any remaining state
        repo.sync_all().map_err(map_error)?;
        Ok(GcResponse {
            removed_checkpoints: stats.removed_checkpoints as usize,
            removed_snapshots: stats.removed_snapshots as usize,
            freed_bytes: stats.freed_bytes,
            delta_chain_depth_triggered: stats.delta_chain_depth_triggered,
        })
    }

    pub fn compact(&self, req: CompactRequest) -> ApiResult<CompactResponse> {
        let mut opts = CompactOptions::from(&self.maintenance_cfg);
        if let Some(vacuum_full) = req.vacuum_full {
            opts.vacuum_full = vacuum_full;
        }
        let report = self
            .storage
            .run_maintenance_with(&opts)
            .map_err(|e| map_error(crate::error::LayertwineError::Storage(e)))?;
        Ok(CompactResponse {
            wal_checkpointed: report.wal_checkpointed,
            freelist_before: report.freelist_before,
            total_pages: report.total_pages,
            freelist_after: report.freelist_after,
            vacuum_performed: report.vacuum_performed,
            message: report.message,
        })
    }

    pub fn git_commit(&self, req: GitCommitRequest) -> ApiResult<GitCommitResponse> {
        let message = req.message.unwrap_or_else(|| "sync from layertwine".into());

        let mut repo = load_checkpoint_repo(self.storage.as_ref()).map_err(map_error)?;
        let branch_name = repo.current_branch_name().to_string();
        let git_hash = GitBridge::push_to_git(
            self.storage.as_ref(),
            Path::new(&req.git_repo),
            &mut repo,
            &branch_name,
            &message,
        )
        .map_err(map_error)?;

        // Persist git_anchor changes
        repo.sync_all().map_err(map_error)?;

        Ok(GitCommitResponse {
            git_commit_hash: git_hash,
        })
    }

    pub fn clean(&self, req: CleanRequest) -> ApiResult<CleanResponse> {
        let mut response = CleanResponse {
            removed_branches: 0,
            removed_checkpoints: 0,
            removed_snapshots: 0,
            removed_deltas: 0,
            removed_layers: 0,
            message: String::new(),
        };

        if req.all {
            // Clean all: remove everything from storage in order (respecting FK constraints)
            let storage = self.storage.as_ref();
            let _ = storage.with_atomic(|s| {
                if let Ok(partitions) = s.list_partitions() {
                    for p in &partitions {
                        let _ = s.delete_partition(&p.id);
                    }
                }
                s.clear_all_checkpoints().ok();
                s.clear_all_branches().ok();
                s.clear_all_layers().ok();
                s.clear_all_snapshots().ok();
                s.clear_all_deltas().ok();
                Ok(())
            });

            response.message = "All layertwine storage cleared. Run `init` to reinitialize.".into();
            return Ok(response);
        }

        // Clean specific branch
        if let Some(branch_name) = &req.branch {
            let mut repo = load_checkpoint_repo(self.storage.as_ref()).map_err(map_error)?;
            let branch_count_before = repo.branches.len();

            if repo.remove_branch(branch_name).is_ok() {
                let stats = collect_garbage(&mut repo).map_err(map_error)?;
                repo.sync_all().map_err(map_error)?;
                response.removed_branches = branch_count_before - repo.branches.len();
                response.removed_checkpoints = stats.removed_checkpoints as usize;
            }
        }

        // Clean specific layer
        if let Some(layer_type_name) = &req.layer {
            let storage = self.storage.as_ref();
            if let Some(lt) = crate::core::types::LayerType::from_name(layer_type_name) {
                if let Ok(layer) = storage.get_layer(&lt) {
                    for pid in &layer.partitions {
                        let _ = storage.delete_partition(pid);
                    }
                }
                storage.delete_layer(&lt).ok();
                response.removed_layers += 1;
            }
        }

        // Clean orphaned snapshots and deltas (SQL-level cleanup)
        let storage = self.storage.as_ref();
        response.removed_snapshots = storage.cleanup_orphan_snapshots().unwrap_or(0);
        response.removed_deltas = storage.cleanup_orphan_deltas().unwrap_or(0);

        if response.removed_branches == 0
            && response.removed_checkpoints == 0
            && response.removed_snapshots == 0
            && response.removed_deltas == 0
            && response.removed_layers == 0
        {
            response.message = "Nothing to clean.".into();
        } else {
            response.message = format!(
                "Cleaned: {} branch(es), {} checkpoint(s), {} snapshot(s), {} delta(s), {} layer(s)",
                response.removed_branches,
                response.removed_checkpoints,
                response.removed_snapshots,
                response.removed_deltas,
                response.removed_layers,
            );
        }

        Ok(response)
    }

    pub fn pull(&self, req: PullRequest) -> ApiResult<PullResponse> {
        let remote = req.remote.unwrap_or_else(|| "origin".into());
        let git_ref = req.git_ref.unwrap_or_else(|| "HEAD".into());

        GitBridge::fetch_from_remote(Path::new(&req.git_repo), &remote).map_err(map_error)?;

        let mut repo = load_checkpoint_repo(self.storage.as_ref()).map_err(map_error)?;

        GitBridge::init_from_git(
            Path::new(&req.git_repo),
            self.storage.as_ref(),
            &mut repo,
            &git_ref,
        )
        .map_err(map_error)?;

        // Auto-persisted via embedded storage; sync any metadata changes
        repo.sync_all().map_err(map_error)?;

        Ok(PullResponse { remote, git_ref })
    }

    pub fn show(&self, req: ShowRequest) -> ApiResult<ShowResponse> {
        match req.show_what.as_str() {
            "staged" => self.show_staged(),
            "checkpoint" => {
                let id = req.target_id.as_deref().ok_or_else(|| {
                    ApiError::invalid_params("checkpoint ID required for 'checkpoint' target")
                })?;
                self.show_checkpoint(id)
            }
            "partition" => {
                let name = req.target_id.as_deref().ok_or_else(|| {
                    ApiError::invalid_params("partition name required for 'partition' target")
                })?;
                self.show_partition(name)
            }
            other => Err(ApiError::invalid_params(format!(
                "unknown show target '{}'. Use 'staged', 'checkpoint', or 'partition'",
                other
            ))),
        }
    }

    // ── Approval-specific API implementations ──

    pub fn list_pending_approvals(&self) -> ApiResult<ListPendingApprovalsResponse> {
        let pending = crate::layered::approval::list_pending_approvals(self.storage.as_ref())
            .map_err(map_error)?;
        let approvals: Vec<ApprovalInfo> = pending
            .iter()
            .map(|p| {
                let agent_id = match &p.partition_type {
                    crate::core::types::PartitionType::Approval(id) => id.0.clone(),
                    _ => String::new(),
                };
                ApprovalInfo {
                    agent_id,
                    partition_name: p.name.clone(),
                    current_snapshot: p.current_snapshot.to_hex(),
                    history_len: p.history.len(),
                }
            })
            .collect();
        let total = approvals.len();
        Ok(ListPendingApprovalsResponse { approvals, total })
    }

    pub fn approve_agent(&self, req: ApproveAgentRequest) -> ApiResult<ApproveAgentResponse> {
        let agent_instance = crate::core::types::AgentInstanceId(req.agent_id.clone());
        let integrated_name = req
            .integrated_name
            .clone()
            .unwrap_or_else(|| req.agent_id.clone());

        let integrated_snapshot_id = crate::layered::integrated::merge_agent_to_feature(
            self.storage.as_ref(),
            &agent_instance,
            &integrated_name,
        )
        .map(|r| r.snapshot_id)
        .map_err(map_error)?;

        Ok(ApproveAgentResponse {
            agent_id: req.agent_id,
            integrated_snapshot_id: snapshot_id_to_hex(&integrated_snapshot_id),
        })
    }

    pub fn reject_agent(&self, req: RejectAgentRequest) -> ApiResult<RejectAgentResponse> {
        let agent_instance = crate::core::types::AgentInstanceId(req.agent_id.clone());
        let baseline_snapshot_id =
            crate::layered::approval::reject_approval(self.storage.as_ref(), &agent_instance)
                .map_err(map_error)?;

        Ok(RejectAgentResponse {
            agent_id: req.agent_id,
            baseline_snapshot_id: snapshot_id_to_hex(&baseline_snapshot_id),
        })
    }

    /// Merge integrated partitions directly to staged (replaces former unified layer).
    /// Kept for backward compatibility — the unified layer has been removed.
    pub fn merge_to_unified(&self, req: MergeToUnifiedRequest) -> ApiResult<MergeToUnifiedResponse> {
        let names = req.integration_names.unwrap_or_else(|| {
            // Auto-detect all integrated partition names
            self.storage
                .list_partitions()
                .ok()
                .map(|partitions| {
                    partitions
                        .into_iter()
                        .filter_map(|p| match p.partition_type {
                            crate::core::types::PartitionType::Integrated(name) => Some(name),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        });

        if names.is_empty() {
            return Err(ApiError::invalid_params(
                "no integrated partitions found to merge",
            ));
        }

        let merged_count = names.len();
        let result =
            crate::layered::staged::merge_features_to_staged(self.storage.as_ref(), &names)
                .map_err(map_error)?;

        Ok(MergeToUnifiedResponse {
            unified_snapshot_id: snapshot_id_to_hex(&result.snapshot_id),
            merged_count,
        })
    }

    /// Merge to staged (now merges integrated features directly).
    /// Kept for backward compatibility.
    pub fn merge_to_staged(&self, _req: MergeToStagedRequest) -> ApiResult<MergeToStagedResponse> {
        // Auto-detect all integrated partition names and merge directly to staged
        let names: Vec<String> = self
            .storage
            .list_partitions()
            .ok()
            .map(|partitions| {
                partitions
                    .into_iter()
                    .filter_map(|p| match p.partition_type {
                        crate::core::types::PartitionType::Integrated(name) => Some(name),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let staged_snapshot_id = if names.is_empty() {
            // No integrated partitions — staged is already up to date
            let staged_pid = crate::layered::staged::staged_partition_id();
            let staged = self.storage.get_partition(&staged_pid).map_err(|e| {
                map_error(LayertwineError::Storage(e))
            })?;
            snapshot_id_to_hex(&staged.current_snapshot)
        } else {
            let result =
                crate::layered::staged::merge_features_to_staged(self.storage.as_ref(), &names)
                    .map_err(map_error)?;
            snapshot_id_to_hex(&result.snapshot_id)
        };

        Ok(MergeToStagedResponse { staged_snapshot_id })
    }
}
