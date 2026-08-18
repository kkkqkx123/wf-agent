#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Storage error in {operation}: {message}")]
    General {
        operation: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Entity not found: type={entity_type}, id={entity_id}")]
    NotFound {
        entity_type: String,
        entity_id: String,
    },

    #[error("Storage quota exceeded: required={required}, available={available}")]
    QuotaExceeded { required: u64, available: u64 },

    #[error("Storage initialization failed: {backend}: {message}")]
    Initialization {
        backend: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Serialization failed for entity {entity}: {message}")]
    Serialization {
        entity: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Integrity check failed for {id}: expected={expected}, actual={actual}")]
    Integrity {
        id: String,
        expected: String,
        actual: String,
    },

    #[error("Connection pool error: {backend}: {message}")]
    Pool { backend: String, message: String },

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Storage state error: expected {expected}, actual {actual}")]
    StateError { expected: String, actual: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        StorageError::Serialization {
            entity: String::new(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => StorageError::NotFound {
                entity_type: String::new(),
                entity_id: String::new(),
            },
            _ => StorageError::General {
                operation: "sqlx".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            },
        }
    }
}
