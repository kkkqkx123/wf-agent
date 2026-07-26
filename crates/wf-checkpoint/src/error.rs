use thiserror::Error;

#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("checkpoint not found: {id}")]
    NotFound { id: String },

    #[error("checkpoint validation failed: {reason}")]
    Validation { reason: String },

    #[error("checkpoint corrupted: id={id}, reason={reason}")]
    Corrupted { id: String, reason: String },

    #[error("storage error: {0}")]
    Storage(#[from] wf_storage::error::StorageError),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("version incompatible: current={current}, required={required}")]
    VersionIncompatible { current: String, required: String },

    #[error("delta chain broken: checkpoint_id={checkpoint_id}, missing={missing_id}")]
    DeltaChainBroken {
        checkpoint_id: String,
        missing_id: String,
    },

    #[error("delta chain too long: length={length}, max={max}")]
    DeltaChainTooLong { length: u32, max: u32 },

    #[error("branch error: {0}")]
    Branch(String),

    #[error("strategy error: {0}")]
    Strategy(String),

    #[error("coordinator error: {0}")]
    Coordinator(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for CheckpointError {
    fn from(e: serde_json::Error) -> Self {
        CheckpointError::Serialization(e.to_string())
    }
}

impl From<bincode::Error> for CheckpointError {
    fn from(e: bincode::Error) -> Self {
        CheckpointError::Serialization(e.to_string())
    }
}
