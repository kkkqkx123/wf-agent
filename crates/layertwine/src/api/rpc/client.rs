//! Layertwine gRPC client library.
//!
//! Wraps the tonic-generated `LayertwineClient` with typed convenience
//! methods for the common repository operations. Enabled with
//! `feature = "grpc"`; the server implementation lives in [`super`].
//!
//! ```no_run
//! use layertwine::api::rpc::client::LayertwineGrpcClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = LayertwineGrpcClient::connect("http://127.0.0.1:50051").await?;
//! let resp = client.init(layertwine::api::rpc::client::InitRequest {
//!     db_path: None,
//!     git_repo: None,
//!     git_ref: None,
//! }).await?;
//! println!("branch: {}", resp.branch);
//! # Ok(())
//! # }
//! ```

use tonic::transport::Channel;
use tonic::{Code, Status};

use super::layertwine_proto::layertwine_client::LayertwineClient;
use super::layertwine_proto::{
    AgentEditRequest, AgentSubmitRequest, ApproveRequest, BackupRequest, BranchCreateRequest,
    BranchListResponse, BranchSwitchRequest, CheckpointDiffRequest, CheckpointDiffResponse,
    CheckpointRestoreRequest, CheckpointRestoreResponse, CommitRequest, EditRequest, EditResponse,
    Empty, InitRequest, InitResponse, LogRequest, LogResponse, RestoreRequest, StatusResponse,
};

pub use super::layertwine_proto::{
    AgentSubmitRequest as AgentSubmit, CheckpointRestoreByTimeRequest,
    CheckpointRestoreRequest as CheckpointRestore,
};

/// Error returned by the Layertwine gRPC client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("gRPC error ({code:?}): {message}")]
    Grpc { code: Code, message: String },
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
}

impl From<Status> for ClientError {
    fn from(status: Status) -> Self {
        ClientError::Grpc {
            code: status.code(),
            message: status.message().to_string(),
        }
    }
}

pub type ClientResult<T> = Result<T, ClientError>;

/// Typed gRPC client for the Layertwine service.
#[derive(Clone)]
pub struct LayertwineGrpcClient {
    inner: LayertwineClient<Channel>,
}

impl LayertwineGrpcClient {
    /// Connect to a Layertwine gRPC server.
    pub async fn connect(address: &str) -> ClientResult<Self> {
        let channel = Channel::from_shared(address.to_string())
            .map_err(|e| ClientError::Grpc {
                code: Code::InvalidArgument,
                message: e.to_string(),
            })?
            .connect()
            .await?;
        Ok(Self::new(channel))
    }

    /// Wrap an existing channel.
    pub fn new(channel: Channel) -> Self {
        Self {
            inner: LayertwineClient::new(channel),
        }
    }

    pub async fn init(&mut self, request: InitRequest) -> ClientResult<InitResponse> {
        Ok(self.inner.init(request).await?.into_inner())
    }

    pub async fn status(&mut self) -> ClientResult<StatusResponse> {
        Ok(self.inner.status(Empty {}).await?.into_inner())
    }

    pub async fn edit(&mut self, request: EditRequest) -> ClientResult<EditResponse> {
        Ok(self.inner.edit(request).await?.into_inner())
    }

    pub async fn agent_edit(&mut self, request: AgentEditRequest) -> ClientResult<EditResponse> {
        Ok(self.inner.agent_edit(request).await?.into_inner())
    }

    pub async fn agent_submit(
        &mut self,
        request: AgentSubmitRequest,
    ) -> ClientResult<super::layertwine_proto::SubmitResponse> {
        Ok(self.inner.agent_submit(request).await?.into_inner())
    }

    pub async fn approve(
        &mut self,
        request: ApproveRequest,
    ) -> ClientResult<super::layertwine_proto::ApproveResponse> {
        Ok(self.inner.approve(request).await?.into_inner())
    }

    pub async fn commit(
        &mut self,
        request: CommitRequest,
    ) -> ClientResult<super::layertwine_proto::CommitResponse> {
        Ok(self.inner.commit(request).await?.into_inner())
    }

    pub async fn log(&mut self, request: LogRequest) -> ClientResult<LogResponse> {
        Ok(self.inner.log(request).await?.into_inner())
    }

    pub async fn branch_create(
        &mut self,
        request: BranchCreateRequest,
    ) -> ClientResult<super::layertwine_proto::BranchCreateResponse> {
        Ok(self.inner.branch_create(request).await?.into_inner())
    }

    pub async fn branch_switch(
        &mut self,
        request: BranchSwitchRequest,
    ) -> ClientResult<super::layertwine_proto::BranchSwitchResponse> {
        Ok(self.inner.branch_switch(request).await?.into_inner())
    }

    pub async fn branch_list(&mut self) -> ClientResult<BranchListResponse> {
        Ok(self.inner.branch_list(Empty {}).await?.into_inner())
    }

    pub async fn backup(
        &mut self,
        request: BackupRequest,
    ) -> ClientResult<super::layertwine_proto::BackupResponse> {
        Ok(self.inner.backup(request).await?.into_inner())
    }

    pub async fn restore(
        &mut self,
        request: RestoreRequest,
    ) -> ClientResult<super::layertwine_proto::RestoreResponse> {
        Ok(self.inner.restore(request).await?.into_inner())
    }

    pub async fn checkpoint_restore(
        &mut self,
        request: CheckpointRestoreRequest,
    ) -> ClientResult<CheckpointRestoreResponse> {
        Ok(self.inner.checkpoint_restore(request).await?.into_inner())
    }

    pub async fn checkpoint_restore_by_time(
        &mut self,
        request: CheckpointRestoreByTimeRequest,
    ) -> ClientResult<CheckpointRestoreResponse> {
        Ok(self
            .inner
            .checkpoint_restore_by_time(request)
            .await?
            .into_inner())
    }

    pub async fn checkpoint_diff(
        &mut self,
        request: CheckpointDiffRequest,
    ) -> ClientResult<CheckpointDiffResponse> {
        Ok(self.inner.checkpoint_diff(request).await?.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_types_are_constructible() {
        // Compile-time sanity for the most common request shapes.
        let init = InitRequest {
            db_path: Some("/tmp/test.db".into()),
            git_repo: None,
            git_ref: None,
        };
        assert_eq!(init.db_path.as_deref(), Some("/tmp/test.db"));

        let edit = EditRequest {
            file: "a.txt".into(),
            content: Some("hello".into()),
        };
        assert_eq!(edit.file, "a.txt");

        let commit = CommitRequest {
            message: "msg".into(),
            author: None,
        };
        assert_eq!(commit.message, "msg");

        let restore = CheckpointRestoreRequest {
            checkpoint_id: "cp-1".into(),
            source_filter: vec![],
        };
        assert_eq!(restore.checkpoint_id, "cp-1");
    }
}
