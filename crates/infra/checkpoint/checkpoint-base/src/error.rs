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

    #[error("merge conflict: actor={actor}, files={files:?}")]
    MergeConflict { actor: String, files: Vec<String> },

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_renders_each_variant_with_context() {
        let e = CheckpointError::NotFound { id: "cp-1".into() };
        assert_eq!(e.to_string(), "checkpoint not found: cp-1");

        let e = CheckpointError::Validation {
            reason: "bad shape".into(),
        };
        assert!(e.to_string().contains("validation failed: bad shape"));

        let e = CheckpointError::Corrupted {
            id: "cp-2".into(),
            reason: "no timestamp".into(),
        };
        assert!(e.to_string().contains("id=cp-2"));
        assert!(e.to_string().contains("no timestamp"));

        let e = CheckpointError::Serialization("boom".into());
        assert!(e.to_string().contains("serialization error: boom"));

        let e = CheckpointError::VersionIncompatible {
            current: "1.0.0".into(),
            required: "2.0.0".into(),
        };
        let s = e.to_string();
        assert!(s.contains("1.0.0") && s.contains("2.0.0"));

        let e = CheckpointError::DeltaChainBroken {
            checkpoint_id: "cp-3".into(),
            missing_id: "cp-0".into(),
        };
        let s = e.to_string();
        assert!(s.contains("cp-3") && s.contains("cp-0"));

        let e = CheckpointError::DeltaChainTooLong {
            length: 11,
            max: 10,
        };
        assert!(e.to_string().contains("length=11") && e.to_string().contains("max=10"));

        let e = CheckpointError::Branch("ref gone".into());
        assert!(e.to_string().contains("branch error: ref gone"));

        let e = CheckpointError::Strategy("rejected".into());
        assert!(e.to_string().contains("strategy error: rejected"));

        let e = CheckpointError::Coordinator("not initialized".into());
        assert!(e.to_string().contains("coordinator error: not initialized"));

        let e = CheckpointError::MergeConflict {
            actor: "a1".into(),
            files: vec!["f.txt".into()],
        };
        assert!(e.to_string().contains("actor=a1") && e.to_string().contains("f.txt"));

        let e = CheckpointError::Internal("impossible state".into());
        assert!(e.to_string().contains("internal: impossible state"));
    }

    #[test]
    fn from_conversions_wrap_into_existing_variants() {
        // StorageError wraps into the Storage variant.
        let storage_err = wf_storage::error::StorageError::General {
            operation: "save".into(),
            message: "disk full".into(),
            source: None,
        };
        let e: CheckpointError = storage_err.into();
        assert!(matches!(e, CheckpointError::Storage(_)));
        assert!(e.to_string().contains("disk full"));

        // io::Error wraps into the Io variant.
        let e: CheckpointError = std::io::Error::other("eof").into();
        assert!(matches!(e, CheckpointError::Io(_)));
        assert!(e.to_string().contains("eof"));

        // serde_json errors map to Serialization.
        let json_err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let e: CheckpointError = json_err.into();
        assert!(matches!(e, CheckpointError::Serialization(_)));

        // bincode errors map to Serialization too.
        let bin_err = bincode::deserialize::<u8>(&[]).unwrap_err();
        let e: CheckpointError = bin_err.into();
        assert!(matches!(e, CheckpointError::Serialization(_)));
    }
}
