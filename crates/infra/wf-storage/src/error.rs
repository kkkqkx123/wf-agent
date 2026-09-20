#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Storage error in {operation}: {message}")]
    General {
        operation: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
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

/// Driver failures are classified so callers can separate retryable pool
/// faults from configuration faults and malformed queries. Row absence is
/// not an error at this layer: callers use `fetch_optional` and receive
/// `Ok(None)` for missing records.
impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => StorageError::Pool {
                backend: "sqlx".into(),
                message: "connection pool unavailable".into(),
            },
            sqlx::Error::RowNotFound => StorageError::InvalidQuery("row not found".into()),
            sqlx::Error::Io(io) => StorageError::Io(io),
            sqlx::Error::Database(db) => StorageError::General {
                operation: "database".into(),
                message: format!("{} ({})", db.message(), db.code().unwrap_or_default()),
                source: None,
            },
            other => {
                let text = other.to_string();
                let lower = text.to_lowercase();
                if lower.contains("connect")
                    || lower.contains("tls")
                    || lower.contains("protocol")
                    || lower.contains("parse")
                {
                    StorageError::Initialization {
                        backend: "sqlx".into(),
                        message: text,
                        source: None,
                    }
                } else if lower.contains("column")
                    || lower.contains("row ")
                    || lower.contains("decode")
                    || lower.contains("type")
                    || lower.contains("argument")
                {
                    StorageError::InvalidQuery(text)
                } else {
                    StorageError::General {
                        operation: "sqlx".into(),
                        message: text,
                        source: None,
                    }
                }
            }
        }
    }
}
