//! gRPC transport layer for Layertwine API
//!
//! Provides a tonic-based gRPC server that wraps the ApiService trait.
//! Enabled with `feature = "grpc"`.
//!
//! Proto schema is compiled at build time via `build.rs` using `tonic-build`.
//! Generated code is included via `tonic::include_proto!("layertwine")`.

pub mod layertwine_proto {
    tonic::include_proto!("layertwine");
}

use std::net::SocketAddr;
use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::api::service::ApiService;
use crate::api::types::*;
use crate::error::LayertwineError;
use layertwine_proto::layertwine_server::{Layertwine, LayertwineServer};

/// gRPC service implementation wrapping ApiService
pub struct LayertwineGrpc {
    service: Arc<ApiService>,
}

impl LayertwineGrpc {
    pub fn new(service: Arc<ApiService>) -> Self {
        Self { service }
    }
}

fn to_status(e: ApiError) -> Status {
    match e.code.as_str() {
        "NOT_FOUND" => Status::not_found(e.message),
        "INVALID_PARAMS" => Status::invalid_argument(e.message),
        "ALREADY_EXISTS" => Status::already_exists(e.message),
        "PERMISSION_DENIED" => Status::permission_denied(e.message),
        _ => Status::internal(format!("[{}] {}", e.code, e.message)),
    }
}

#[tonic::async_trait]
impl Layertwine for LayertwineGrpc {
    async fn init(
        &self,
        request: Request<layertwine_proto::InitRequest>,
    ) -> Result<Response<layertwine_proto::InitResponse>, Status> {
        let req = request.into_inner();
        let api_req = InitRequest {
            db_path: req.db_path,
            git_repo: req.git_repo,
            git_ref: req.git_ref,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.init(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::InitResponse {
            db_path: result.db_path,
            manual_partition_id: result.manual_partition_id,
            staged_partition_id: result.staged_partition_id,
            branch: result.branch,
        }))
    }

    async fn status(
        &self,
        _request: Request<layertwine_proto::Empty>,
    ) -> Result<Response<layertwine_proto::StatusResponse>, Status> {
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.status())
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let partitions = result
            .partitions
            .into_iter()
            .map(|p| layertwine_proto::PartitionInfo {
                layer: p.layer,
                name: p.name,
                current_snapshot: p.current_snapshot,
                history_len: p.history_len as u32,
            })
            .collect();
        Ok(Response::new(layertwine_proto::StatusResponse {
            partitions,
        }))
    }

    async fn edit(
        &self,
        request: Request<layertwine_proto::EditRequest>,
    ) -> Result<Response<layertwine_proto::EditResponse>, Status> {
        let req = request.into_inner();
        let api_req = EditRequest {
            file: req.file,
            content: req.content,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.edit(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::EditResponse {
            snapshot_id: result.snapshot_id,
            staged_snapshot_id: result.staged_snapshot_id,
        }))
    }

    async fn agent_edit(
        &self,
        request: Request<layertwine_proto::AgentEditRequest>,
    ) -> Result<Response<layertwine_proto::EditResponse>, Status> {
        let req = request.into_inner();
        let api_req = AgentEditRequest {
            agent_id: req.agent_id,
            file: req.file,
            content: req.content,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.agent_edit(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::EditResponse {
            snapshot_id: result.snapshot_id,
            staged_snapshot_id: result.staged_snapshot_id,
        }))
    }

    async fn agent_submit(
        &self,
        request: Request<layertwine_proto::AgentSubmitRequest>,
    ) -> Result<Response<layertwine_proto::SubmitResponse>, Status> {
        let req = request.into_inner();
        let api_req = AgentSubmitRequest {
            agent_id: req.agent_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.agent_submit(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::SubmitResponse {
            snapshot_id: result.snapshot_id,
        }))
    }

    async fn approve(
        &self,
        request: Request<layertwine_proto::ApproveRequest>,
    ) -> Result<Response<layertwine_proto::ApproveResponse>, Status> {
        let req = request.into_inner();
        let api_req = ApproveRequest {
            agent_id: req.agent_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.approve(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::ApproveResponse {
            integrated_snapshot_id: result.integrated_snapshot_id,
            staged_snapshot_id: result.staged_snapshot_id,
        }))
    }

    async fn commit(
        &self,
        request: Request<layertwine_proto::CommitRequest>,
    ) -> Result<Response<layertwine_proto::CommitResponse>, Status> {
        let req = request.into_inner();
        let api_req = CommitRequest {
            message: req.message,
            author: req.author,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.commit(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::CommitResponse {
            checkpoint_id: result.checkpoint_id,
            message: result.message,
        }))
    }

    async fn log(
        &self,
        request: Request<layertwine_proto::LogRequest>,
    ) -> Result<Response<layertwine_proto::LogResponse>, Status> {
        let req = request.into_inner();
        let api_req = LogRequest {
            count: req.count.map(|c| c as usize),
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.log(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let checkpoints = result
            .checkpoints
            .into_iter()
            .map(|cp| layertwine_proto::CheckpointInfo {
                id: cp.id,
                author: cp.author,
                message: cp.message,
                parents: cp.parents,
                snapshots: cp.snapshots,
                created_at: cp.created_at,
                git_anchor: cp.git_anchor,
            })
            .collect();
        Ok(Response::new(layertwine_proto::LogResponse {
            checkpoints,
            total: result.total as u32,
        }))
    }

    async fn branch_create(
        &self,
        request: Request<layertwine_proto::BranchCreateRequest>,
    ) -> Result<Response<layertwine_proto::BranchCreateResponse>, Status> {
        let req = request.into_inner();
        let api_req = BranchCreateRequest { name: req.name };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.branch_create(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::BranchCreateResponse {
            name: result.name,
            head: result.head,
        }))
    }

    async fn branch_switch(
        &self,
        request: Request<layertwine_proto::BranchSwitchRequest>,
    ) -> Result<Response<layertwine_proto::BranchSwitchResponse>, Status> {
        let req = request.into_inner();
        let api_req = BranchSwitchRequest { name: req.name };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.branch_switch(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::BranchSwitchResponse {
            name: result.name,
            checkpoint_id: result.checkpoint_id,
        }))
    }

    async fn branch_list(
        &self,
        _request: Request<layertwine_proto::Empty>,
    ) -> Result<Response<layertwine_proto::BranchListResponse>, Status> {
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.branch_list())
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let branches = result
            .branches
            .into_iter()
            .map(|b| layertwine_proto::BranchInfo {
                name: b.name,
                head: b.head,
                updated_at: b.updated_at,
                is_current: b.is_current,
            })
            .collect();
        Ok(Response::new(layertwine_proto::BranchListResponse {
            branches,
            current: result.current,
        }))
    }

    async fn merge(
        &self,
        request: Request<layertwine_proto::MergeRequest>,
    ) -> Result<Response<layertwine_proto::MergeResponse>, Status> {
        let req = request.into_inner();
        let api_req = MergeRequest {
            branch: req.branch,
            message: req.message,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.merge(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::MergeResponse {
            checkpoint_id: result.checkpoint_id,
            source_branch: result.source_branch,
            target_branch: result.target_branch,
        }))
    }

    async fn backup(
        &self,
        request: Request<layertwine_proto::BackupRequest>,
    ) -> Result<Response<layertwine_proto::BackupResponse>, Status> {
        let req = request.into_inner();
        let api_req = BackupRequest {
            snapshot_id: req.snapshot_id,
            label: req.label,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.backup(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::BackupResponse {
            backup_id: result.backup_id,
            source_snapshot_id: result.source_snapshot_id,
            label: result.label,
        }))
    }

    async fn restore(
        &self,
        request: Request<layertwine_proto::RestoreRequest>,
    ) -> Result<Response<layertwine_proto::RestoreResponse>, Status> {
        let req = request.into_inner();
        let api_req = RestoreRequest {
            backup_id: req.backup_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.restore(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::RestoreResponse {
            backup_id: result.backup_id,
            file: result.file,
            deltas_restored: result.deltas_restored as u32,
            merged_snapshot_id: result.merged_snapshot_id,
        }))
    }

    async fn gc(
        &self,
        _request: Request<layertwine_proto::Empty>,
    ) -> Result<Response<layertwine_proto::GcResponse>, Status> {
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.gc(GcRequest {}))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::GcResponse {
            removed_checkpoints: result.removed_checkpoints as u32,
            removed_snapshots: result.removed_snapshots as u32,
            freed_bytes: result.freed_bytes,
            delta_chain_depth_triggered: result.delta_chain_depth_triggered,
        }))
    }

    async fn git_commit(
        &self,
        request: Request<layertwine_proto::GitCommitRequest>,
    ) -> Result<Response<layertwine_proto::GitCommitResponse>, Status> {
        let req = request.into_inner();
        let api_req = GitCommitRequest {
            git_repo: req.git_repo,
            message: req.message,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.git_commit(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::GitCommitResponse {
            git_commit_hash: result.git_commit_hash,
        }))
    }

    async fn clean(
        &self,
        request: Request<layertwine_proto::CleanRequest>,
    ) -> Result<Response<layertwine_proto::CleanResponse>, Status> {
        let req = request.into_inner();
        let api_req = CleanRequest {
            branch: req.branch,
            layer: req.layer,
            all: req.all,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.clean(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::CleanResponse {
            removed_branches: result.removed_branches as u32,
            removed_checkpoints: result.removed_checkpoints as u32,
            removed_snapshots: result.removed_snapshots as u32,
            removed_deltas: result.removed_deltas as u32,
            removed_layers: result.removed_layers as u32,
            message: result.message,
        }))
    }

    async fn pull(
        &self,
        request: Request<layertwine_proto::PullRequest>,
    ) -> Result<Response<layertwine_proto::PullResponse>, Status> {
        let req = request.into_inner();
        let api_req = PullRequest {
            remote: req.remote,
            git_repo: req.git_repo,
            git_ref: req.git_ref,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.pull(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::PullResponse {
            remote: result.remote,
            git_ref: result.git_ref,
        }))
    }

    async fn compact(
        &self,
        request: Request<layertwine_proto::CompactRequest>,
    ) -> Result<Response<layertwine_proto::CompactResponse>, Status> {
        let req = request.into_inner();
        let api_req = CompactRequest {
            vacuum_full: req.vacuum_full,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.compact(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::CompactResponse {
            wal_checkpointed: result.wal_checkpointed,
            freelist_before: result.freelist_before,
            total_pages: result.total_pages,
            freelist_after: result.freelist_after,
            vacuum_performed: result.vacuum_performed,
            message: result.message,
        }))
    }

    async fn show(
        &self,
        request: Request<layertwine_proto::ShowRequest>,
    ) -> Result<Response<layertwine_proto::ShowResponse>, Status> {
        let req = request.into_inner();
        let api_req = ShowRequest {
            show_what: req.show_what,
            target_id: req.target_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.show(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let diffs = result
            .diffs
            .into_iter()
            .map(|d| layertwine_proto::FileDiff {
                file_path: d.file_path,
                unified_diff: d.unified_diff,
                inserts: d.inserts as u32,
                deletes: d.deletes as u32,
            })
            .collect();
        Ok(Response::new(layertwine_proto::ShowResponse {
            target: result.target,
            diffs,
        }))
    }

    async fn checkpoint_restore(
        &self,
        request: Request<layertwine_proto::CheckpointRestoreRequest>,
    ) -> Result<Response<layertwine_proto::CheckpointRestoreResponse>, Status> {
        let req = request.into_inner();
        let api_req = CheckpointRestoreRequest {
            checkpoint_id: req.checkpoint_id,
            source_filter: if req.source_filter.is_empty() {
                None
            } else {
                Some(req.source_filter)
            },
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.checkpoint_restore(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let snapshots = result
            .snapshots
            .into_iter()
            .map(|s| layertwine_proto::RestoredSnapshotInfo {
                snapshot_id: s.snapshot_id,
                source: s.source,
                content_hex: s.content_hex,
                content_type: s.content_type,
            })
            .collect();
        Ok(Response::new(layertwine_proto::CheckpointRestoreResponse {
            checkpoint: Some(layertwine_proto::CheckpointInfo {
                id: result.checkpoint.id,
                author: result.checkpoint.author,
                message: result.checkpoint.message,
                parents: result.checkpoint.parents,
                snapshots: result.checkpoint.snapshots,
                created_at: result.checkpoint.created_at,
                git_anchor: result.checkpoint.git_anchor,
            }),
            snapshots,
            ancestry: result.ancestry,
        }))
    }

    async fn checkpoint_restore_by_time(
        &self,
        request: Request<layertwine_proto::CheckpointRestoreByTimeRequest>,
    ) -> Result<Response<layertwine_proto::CheckpointRestoreResponse>, Status> {
        let req = request.into_inner();
        let api_req = CheckpointRestoreByTimeRequest {
            target_time: req.target_time,
            source_filter: if req.source_filter.is_empty() {
                None
            } else {
                Some(req.source_filter)
            },
        };
        let service = self.service.clone();
        let result =
            tokio::task::spawn_blocking(move || service.checkpoint_restore_by_time(api_req))
                .await
                .map_err(|e| Status::internal(format!("join error: {}", e)))?
                .map_err(to_status)?;
        let snapshots = result
            .snapshots
            .into_iter()
            .map(|s| layertwine_proto::RestoredSnapshotInfo {
                snapshot_id: s.snapshot_id,
                source: s.source,
                content_hex: s.content_hex,
                content_type: s.content_type,
            })
            .collect();
        Ok(Response::new(layertwine_proto::CheckpointRestoreResponse {
            checkpoint: Some(layertwine_proto::CheckpointInfo {
                id: result.checkpoint.id,
                author: result.checkpoint.author,
                message: result.checkpoint.message,
                parents: result.checkpoint.parents,
                snapshots: result.checkpoint.snapshots,
                created_at: result.checkpoint.created_at,
                git_anchor: result.checkpoint.git_anchor,
            }),
            snapshots,
            ancestry: result.ancestry,
        }))
    }

    async fn checkpoint_diff(
        &self,
        request: Request<layertwine_proto::CheckpointDiffRequest>,
    ) -> Result<Response<layertwine_proto::CheckpointDiffResponse>, Status> {
        let req = request.into_inner();
        let api_req = CheckpointDiffRequest {
            from_id: req.from_id,
            to_id: req.to_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.checkpoint_diff(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::CheckpointDiffResponse {
            from_id: result.from_id,
            to_id: result.to_id,
            added: result.added,
            removed: result.removed,
            modified: result.modified,
            total_changes: result.total_changes as u32,
        }))
    }

    async fn checkpoint_rollback(
        &self,
        request: Request<layertwine_proto::CheckpointRollbackRequest>,
    ) -> Result<Response<layertwine_proto::CheckpointRollbackResponse>, Status> {
        let req = request.into_inner();
        let api_req = CheckpointRollbackRequest {
            checkpoint_id: req.checkpoint_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.checkpoint_rollback(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(
            layertwine_proto::CheckpointRollbackResponse {
                checkpoint_id: result.checkpoint_id,
                snapshot_ids: result.snapshot_ids,
            },
        ))
    }

    async fn list_pending_approvals(
        &self,
        _request: Request<layertwine_proto::Empty>,
    ) -> Result<Response<layertwine_proto::ListPendingApprovalsResponse>, Status> {
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.list_pending_approvals())
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        let approvals = result
            .approvals
            .into_iter()
            .map(|a| layertwine_proto::ApprovalInfo {
                agent_id: a.agent_id,
                partition_name: a.partition_name,
                current_snapshot: a.current_snapshot,
                history_len: a.history_len as u32,
            })
            .collect();
        Ok(Response::new(
            layertwine_proto::ListPendingApprovalsResponse {
                approvals,
                total: result.total as u32,
            },
        ))
    }

    async fn approve_agent(
        &self,
        request: Request<layertwine_proto::ApproveAgentRequest>,
    ) -> Result<Response<layertwine_proto::ApproveAgentResponse>, Status> {
        let req = request.into_inner();
        let api_req = ApproveAgentRequest {
            agent_id: req.agent_id,
            integrated_name: req.integrated_name,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.approve_agent(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::ApproveAgentResponse {
            agent_id: result.agent_id,
            integrated_snapshot_id: result.integrated_snapshot_id,
        }))
    }

    async fn reject_agent(
        &self,
        request: Request<layertwine_proto::RejectAgentRequest>,
    ) -> Result<Response<layertwine_proto::RejectAgentResponse>, Status> {
        let req = request.into_inner();
        let api_req = RejectAgentRequest {
            agent_id: req.agent_id,
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.reject_agent(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::RejectAgentResponse {
            agent_id: result.agent_id,
            baseline_snapshot_id: result.baseline_snapshot_id,
        }))
    }

    async fn merge_to_unified(
        &self,
        request: Request<layertwine_proto::MergeToUnifiedRequest>,
    ) -> Result<Response<layertwine_proto::MergeToUnifiedResponse>, Status> {
        let req = request.into_inner();
        let api_req = MergeToUnifiedRequest {
            integration_names: if req.integration_names.is_empty() {
                None
            } else {
                Some(req.integration_names)
            },
        };
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.merge_to_unified(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::MergeToUnifiedResponse {
            unified_snapshot_id: result.unified_snapshot_id,
            merged_count: result.merged_count as u32,
        }))
    }

    async fn merge_to_staged(
        &self,
        request: Request<layertwine_proto::MergeToStagedRequest>,
    ) -> Result<Response<layertwine_proto::MergeToStagedResponse>, Status> {
        let _req = request.into_inner();
        let api_req = MergeToStagedRequest {};
        let service = self.service.clone();
        let result = tokio::task::spawn_blocking(move || service.merge_to_staged(api_req))
            .await
            .map_err(|e| Status::internal(format!("join error: {}", e)))?
            .map_err(to_status)?;
        Ok(Response::new(layertwine_proto::MergeToStagedResponse {
            staged_snapshot_id: result.staged_snapshot_id,
        }))
    }
}

/// Start the gRPC server
///
/// ```no_run
/// use std::sync::Arc;
/// use std::net::SocketAddr;
/// use layertwine::api::service::{ApiService, ServiceConfig};
/// use layertwine::api::rpc;
///
/// # async fn example() {
/// let service = ApiService::open(ServiceConfig::default()).unwrap();
/// let addr: SocketAddr = "127.0.0.1:50051".parse().unwrap();
/// rpc::serve(Arc::new(service), addr).await.unwrap();
/// # }
/// ```
pub async fn serve(service: Arc<ApiService>, addr: SocketAddr) -> Result<(), LayertwineError> {
    let grpc = LayertwineGrpc::new(service);

    tonic::transport::Server::builder()
        .add_service(LayertwineServer::new(grpc))
        .serve(addr)
        .await
        .map_err(|e| LayertwineError::General(format!("gRPC server error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── to_status error mapping tests ──

    #[test]
    fn test_to_status_not_found() {
        let err = ApiError::not_found("partition 'test'");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(status.message().contains("partition 'test'"));
    }

    #[test]
    fn test_to_status_invalid_params() {
        let err = ApiError::invalid_params("missing file path");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("missing file path"));
    }

    #[test]
    fn test_to_status_internal() {
        let err = ApiError::internal("database corruption");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("database corruption"));
    }

    #[test]
    fn test_to_status_storage_error() {
        let err = ApiError::storage("disk full");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("disk full"));
    }

    #[test]
    fn test_to_status_engine_error() {
        let err = ApiError::engine("diff failed");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("[ENGINE_ERROR] diff failed"));
    }

    #[test]
    fn test_to_status_state_machine_error() {
        let err = ApiError::state_machine("invalid transition");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("invalid transition"));
    }

    #[test]
    fn test_to_status_checkpoint_error() {
        let err = ApiError::checkpoint("no changes to commit");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("no changes to commit"));
    }

    #[test]
    fn test_to_status_git_sync_error() {
        let err = ApiError::git_sync("remote rejected");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("remote rejected"));
    }

    #[test]
    fn test_to_status_gc_error() {
        let err = ApiError::gc("garbage collection failed");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("garbage collection failed"));
    }

    #[test]
    fn test_to_status_general_error() {
        let err = ApiError::general("unknown error");
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("[ERROR] unknown error"));
    }

    #[test]
    fn test_to_status_already_exists() {
        let err = ApiError {
            code: "ALREADY_EXISTS".into(),
            message: "branch 'feature' already exists".into(),
            suggestion: Some("use a different name".into()),
            details: None,
        };
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::AlreadyExists);
        assert!(status.message().contains("branch 'feature' already exists"));
    }

    #[test]
    fn test_to_status_permission_denied() {
        let err = ApiError {
            code: "PERMISSION_DENIED".into(),
            message: "access denied".into(),
            suggestion: None,
            details: None,
        };
        let status = to_status(err);
        assert_eq!(status.code(), tonic::Code::PermissionDenied);
    }

    // ── LayertwineGrpc construction test ──

    #[test]
    fn test_layertwine_grpc_new() {
        // Full gRPC business logic coverage is in tests/grpc_integration.rs.
    }
}
