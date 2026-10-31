/// Raw OS error number for a full filesystem on Unix. Compared directly so
/// the classifier works without an extra libc dependency.
const ENOSPC_RAW_OS_ERROR: i32 = 28;

/// True when free-form storage error text describes an exhausted disk.
/// Covers both SQLite phrasing and OS phrasing so errors built before the
/// typed mapping still classify correctly.
fn message_reports_disk_full(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("database or disk is full")
        || lower.contains("disk is full")
        || lower.contains("disk full")
        || lower.contains("no space left")
        || lower.contains("no space")
        || lower.contains("enospc")
        || lower.contains("sqlite_full")
}

/// Primary code of a short numeric SQLite result code. Extended codes
/// share the low byte with their primary (5 busy, 6 locked, 13 full,
/// 19 constraint), so masking recovers the primary from either form.
fn sqlite_primary(code: &str) -> Option<i32> {
    let text = code.trim();
    if text.len() <= 4 && !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        text.parse::<i32>().ok().map(|n| n & 0xff)
    } else {
        None
    }
}

/// True when a database driver error code is the SQLite full code (primary
/// code 13; extended codes share the low byte).
fn code_reports_sqlite_full(code: &str) -> bool {
    sqlite_primary(code).map(|n| n == 13).unwrap_or(false) || message_reports_disk_full(code)
}

/// True when a SQLite result code reports a busy or locked database
/// (primary code 5 or 6). The held lock is transient: a later attempt on
/// a fresh connection may succeed.
fn code_reports_sqlite_transient(code: &str) -> bool {
    sqlite_primary(code)
        .map(|n| n == 5 || n == 6)
        .unwrap_or(false)
}

/// True when a SQLite result code is a bare constraint primary (19).
/// Extended constraint codes are classified through the driver-reported
/// error kind instead, which is more precise.
fn code_reports_sqlite_constraint(code: &str) -> bool {
    sqlite_primary(code).map(|n| n == 19).unwrap_or(false)
}

/// True when a driver error code is a five-character SQLSTATE (the
/// Postgres family). Short numeric SQLite codes never reach this branch,
/// so the two namespaces stay disjoint.
fn is_sql_state(code: &str) -> bool {
    let text = code.trim();
    text.len() == 5 && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Classify a SQLSTATE-carrying failure. Connection loss, serialization
/// failures, deadlocks and unavailable locks are transient pool faults;
/// constraint and syntax failures are caller faults; bad authorization is
/// a startup fault. Anything else stays unclassified.
enum SqlStateAction {
    Pool,
    InvalidQuery,
    Initialization,
}

fn classify_sql_state(code: &str) -> Option<SqlStateAction> {
    if !is_sql_state(code) {
        return None;
    }
    let text = code.trim();
    if text.starts_with("08") {
        return Some(SqlStateAction::Pool);
    }
    match text {
        "40001" | "40P01" | "55P03" => Some(SqlStateAction::Pool),
        _ => None,
    }
    .or_else(|| {
        if text.starts_with("28") {
            Some(SqlStateAction::Initialization)
        } else if text.starts_with("23") || text.starts_with("42") {
            Some(SqlStateAction::InvalidQuery)
        } else {
            None
        }
    })
}

/// Classify a database-reported failure for one storage operation.
/// Disk-full is checked first so exhaustion is never mistaken for a
/// transient or caller fault; driver-normalized constraint kinds come
/// next; backend code namespaces (SQLite numerics, SQLSTATE) follow.
fn database_error(db: &dyn sqlx::error::DatabaseError, operation: &str) -> StorageError {
    let message = format!(
        "{} ({})",
        db.message(),
        db.code().as_deref().unwrap_or_default()
    );
    if db
        .code()
        .as_deref()
        .map(code_reports_sqlite_full)
        .unwrap_or(false)
        || message_reports_disk_full(db.message())
    {
        return StorageError::DiskFull {
            operation: operation.into(),
            message,
            source: None,
        };
    }
    match db.kind() {
        sqlx::error::ErrorKind::UniqueViolation
        | sqlx::error::ErrorKind::ForeignKeyViolation
        | sqlx::error::ErrorKind::NotNullViolation
        | sqlx::error::ErrorKind::CheckViolation => {
            return StorageError::InvalidQuery(message);
        }
        _ => {}
    }
    if let Some(code) = db.code() {
        if code_reports_sqlite_transient(&code) {
            return StorageError::Pool {
                backend: "sqlx".into(),
                message: format!("database is busy or locked, retry may succeed: {message}"),
            };
        }
        if code_reports_sqlite_constraint(&code) {
            return StorageError::InvalidQuery(message);
        }
        match classify_sql_state(&code) {
            Some(SqlStateAction::Pool) => {
                return StorageError::Pool {
                    backend: "sqlx".into(),
                    message: format!("transient database fault, retry may succeed: {message}"),
                };
            }
            Some(SqlStateAction::InvalidQuery) => return StorageError::InvalidQuery(message),
            Some(SqlStateAction::Initialization) => {
                return StorageError::Initialization {
                    backend: "sqlx".into(),
                    message,
                    source: None,
                };
            }
            None => {}
        }
    }
    StorageError::General {
        operation: operation.into(),
        message,
        source: None,
    }
}

/// True when an IO error signals an exhausted filesystem.
fn io_reports_disk_full(io: &std::io::Error) -> bool {
    if io.kind() == std::io::ErrorKind::StorageFull {
        return true;
    }
    if io.raw_os_error() == Some(ENOSPC_RAW_OS_ERROR) {
        return true;
    }
    message_reports_disk_full(&io.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Storage error in {operation}: {message}")]
    General {
        operation: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// The filesystem backing the store ran out of space. Retrying without
    /// freeing space cannot succeed; callers should surface the failure and
    /// pause instead of looping.
    #[error("Storage disk full in {operation}: {message}")]
    DiskFull {
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

impl StorageError {
    /// Build a disk-full error for one storage operation.
    pub fn disk_full(operation: impl Into<String>, message: impl Into<String>) -> Self {
        StorageError::DiskFull {
            operation: operation.into(),
            message: message.into(),
            source: None,
        }
    }

    /// True when the failure means the backing filesystem ran out of space,
    /// whether reported by the typed variant, an ENOSPC io error, or legacy
    /// free-form text.
    pub fn is_disk_full(&self) -> bool {
        match self {
            StorageError::DiskFull { .. } => true,
            StorageError::Io(io) => io_reports_disk_full(io),
            StorageError::General { message, .. } => message_reports_disk_full(message),
            _ => false,
        }
    }

    /// True when no more bytes can be persisted: physical exhaustion or a
    /// logical quota. Disk-full callers can narrow with `is_disk_full`.
    pub fn is_storage_exhausted(&self) -> bool {
        matches!(self, StorageError::QuotaExceeded { .. }) || self.is_disk_full()
    }

    /// True when retrying the same operation may succeed without any code
    /// or configuration change: pool exhaustion, a crashed worker, a
    /// failed transaction begin, or a busy/locked database. Every other
    /// failure needs an operator action or a fix first, so blind retries
    /// would only burn tokens.
    pub fn is_retryable(&self) -> bool {
        matches!(self, StorageError::Pool { .. })
    }

    /// Classify a sqlx driver failure for one storage operation. The shared
    /// entry point so CRUD paths keep the disk-full signal instead of
    /// flattening every failure into the generic variant.
    pub fn from_sqlx_with_operation(e: sqlx::Error, operation: &str) -> Self {
        let classified: StorageError = e.into();
        match classified {
            StorageError::General {
                message, source, ..
            } => StorageError::General {
                operation: operation.into(),
                message,
                source,
            },
            StorageError::DiskFull {
                message, source, ..
            } => StorageError::DiskFull {
                operation: operation.into(),
                message,
                source,
            },
            other => other,
        }
    }
}

/// Driver failures are classified so callers can separate retryable pool
/// faults from configuration faults and malformed queries. Every known
/// driver variant maps by name; only future variants unknown to this
/// sqlx version fall back to text matching. Row absence is not an error
/// at this layer: callers use `fetch_optional` and receive `Ok(None)`
/// for missing records.
impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::PoolTimedOut => StorageError::Pool {
                backend: "sqlx".into(),
                message: "connection pool timed out waiting for an open connection".into(),
            },
            sqlx::Error::PoolClosed => StorageError::Pool {
                backend: "sqlx".into(),
                message: "connection pool closed while waiting for an open connection".into(),
            },
            sqlx::Error::WorkerCrashed => StorageError::Pool {
                backend: "sqlx".into(),
                message: "background worker crashed while communicating with the database".into(),
            },
            sqlx::Error::BeginFailed => StorageError::Pool {
                backend: "sqlx".into(),
                message: "failed to begin transaction: unexpected connection status".into(),
            },
            sqlx::Error::RowNotFound => StorageError::InvalidQuery("row not found".into()),
            sqlx::Error::Configuration(source) => StorageError::Initialization {
                backend: "sqlx".into(),
                message: format!("error with configuration: {source}"),
                source: None,
            },
            sqlx::Error::Tls(source) => StorageError::Initialization {
                backend: "sqlx".into(),
                message: format!("tls connection failed: {source}"),
                source: None,
            },
            sqlx::Error::InvalidArgument(detail) => StorageError::InvalidQuery(detail),
            sqlx::Error::TypeNotFound { type_name } => {
                StorageError::InvalidQuery(format!("type named {type_name} not found"))
            }
            sqlx::Error::ColumnIndexOutOfBounds { index, len } => StorageError::InvalidQuery(
                format!("column index out of bounds: the len is {len}, but the index is {index}"),
            ),
            sqlx::Error::ColumnNotFound(name) => {
                StorageError::InvalidQuery(format!("no column found for name: {name}"))
            }
            sqlx::Error::ColumnDecode { index, source } => StorageError::InvalidQuery(format!(
                "error occurred while decoding column {index}: {source}"
            )),
            sqlx::Error::Encode(source) => StorageError::InvalidQuery(format!(
                "error occurred while encoding a value: {source}"
            )),
            sqlx::Error::Decode(source) => {
                StorageError::InvalidQuery(format!("error occurred while decoding: {source}"))
            }
            sqlx::Error::InvalidSavePointStatement => StorageError::InvalidQuery(
                "attempted to call begin_with at non-zero transaction depth".into(),
            ),
            sqlx::Error::Protocol(detail) => StorageError::General {
                operation: "sqlx".into(),
                message: format!("encountered unexpected or invalid data: {detail}"),
                source: None,
            },
            sqlx::Error::AnyDriverError(source) => StorageError::General {
                operation: "sqlx".into(),
                message: format!("error in driver mapping: {source}"),
                source: None,
            },
            sqlx::Error::Io(io) => {
                if io_reports_disk_full(&io) {
                    StorageError::DiskFull {
                        operation: "database".into(),
                        message: io.to_string(),
                        source: Some(Box::new(io)),
                    }
                } else {
                    StorageError::Io(io)
                }
            }
            sqlx::Error::Database(db) => database_error(db.as_ref(), "database"),
            other => {
                // `sqlx::Error` is non-exhaustive: variants added by a
                // future sqlx land here. Disk-full text still classifies;
                // migrations never run through this path (schema is plain
                // DDL), so the remainder is genuinely unknown.
                let text = other.to_string();
                if message_reports_disk_full(&text) {
                    return StorageError::DiskFull {
                        operation: "sqlx".into(),
                        message: text,
                        source: None,
                    };
                }
                StorageError::General {
                    operation: "sqlx".into(),
                    message: text,
                    source: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_disk_full_classifies() {
        let err = StorageError::disk_full("save", "database or disk is full");
        assert!(err.is_disk_full());
        assert!(err.is_storage_exhausted());
    }

    #[test]
    fn enospc_io_classifies_as_disk_full() {
        let io = std::io::Error::from_raw_os_error(ENOSPC_RAW_OS_ERROR);
        assert!(io_reports_disk_full(&io));
        let err = StorageError::Io(io);
        assert!(err.is_disk_full());
        assert!(err.is_storage_exhausted());
    }

    #[test]
    fn storage_full_kind_classifies_as_disk_full() {
        let io = std::io::Error::new(std::io::ErrorKind::StorageFull, "out of space");
        let err = StorageError::Io(io);
        assert!(err.is_disk_full());
    }

    #[test]
    fn legacy_disk_full_text_still_classifies() {
        let err = StorageError::General {
            operation: "save".into(),
            message: "database or disk is full (13)".into(),
            source: None,
        };
        assert!(err.is_disk_full());
    }

    #[test]
    fn unrelated_errors_do_not_classify() {
        let readonly = StorageError::General {
            operation: "save".into(),
            message: "attempt to write a readonly database (8)".into(),
            source: None,
        };
        assert!(!readonly.is_disk_full());
        assert!(!readonly.is_storage_exhausted());
        let quota = StorageError::QuotaExceeded {
            required: 10,
            available: 2,
        };
        assert!(!quota.is_disk_full());
        assert!(quota.is_storage_exhausted());
    }

    #[test]
    fn sqlite_full_codes_match_primary_code() {
        assert!(code_reports_sqlite_full("13"));
        assert!(!code_reports_sqlite_full("8"));
    }

    #[test]
    fn sqlx_operation_mapping_keeps_operation_name() {
        let err = StorageError::from_sqlx_with_operation(sqlx::Error::RowNotFound, "load");
        assert!(matches!(err, StorageError::InvalidQuery(_)));
        assert!(!err.is_disk_full());
    }

    #[test]
    fn pool_faults_are_retryable_and_distinct() {
        for err in [
            sqlx::Error::PoolTimedOut,
            sqlx::Error::PoolClosed,
            sqlx::Error::WorkerCrashed,
            sqlx::Error::BeginFailed,
        ] {
            let err: StorageError = err.into();
            assert!(matches!(err, StorageError::Pool { .. }), "{err:?}");
            assert!(err.is_retryable());
            assert!(!err.is_disk_full());
        }
    }

    #[test]
    fn configuration_faults_are_initialization() {
        let err: StorageError =
            sqlx::Error::Configuration(Box::new(std::io::Error::other("bad url"))).into();
        assert!(matches!(err, StorageError::Initialization { .. }));
        assert!(!err.is_retryable());
        let err: StorageError =
            sqlx::Error::Tls(Box::new(std::io::Error::other("handshake"))).into();
        assert!(matches!(err, StorageError::Initialization { .. }));
    }

    #[test]
    fn programming_faults_are_invalid_query() {
        let cases = [
            sqlx::Error::InvalidArgument("bad bind".into()),
            sqlx::Error::TypeNotFound {
                type_name: "foo".into(),
            },
            sqlx::Error::ColumnIndexOutOfBounds { index: 5, len: 1 },
            sqlx::Error::ColumnNotFound("bar".into()),
            sqlx::Error::ColumnDecode {
                index: "0".into(),
                source: Box::new(std::io::Error::other("null")),
            },
            sqlx::Error::Encode(Box::new(std::io::Error::other("unsupported"))),
            sqlx::Error::Decode(Box::new(std::io::Error::other("overflow"))),
            sqlx::Error::InvalidSavePointStatement,
        ];
        for err in cases {
            let err: StorageError = err.into();
            assert!(matches!(err, StorageError::InvalidQuery(_)), "{err:?}");
            assert!(!err.is_retryable());
            assert!(!err.is_disk_full());
        }
    }

    #[test]
    fn driver_faults_stay_general() {
        let err: StorageError = sqlx::Error::Protocol("bad packet".into()).into();
        assert!(matches!(err, StorageError::General { .. }));
        assert!(!err.is_retryable());
        let err: StorageError =
            sqlx::Error::AnyDriverError(Box::new(std::io::Error::other("boom"))).into();
        assert!(matches!(err, StorageError::General { .. }));
    }

    #[test]
    fn non_pool_failures_are_not_retryable() {
        assert!(!StorageError::disk_full("save", "full").is_retryable());
        assert!(!StorageError::QuotaExceeded {
            required: 1,
            available: 0
        }
        .is_retryable());
        assert!(!StorageError::InvalidQuery("row not found".into()).is_retryable());
        assert!(!StorageError::General {
            operation: "save".into(),
            message: "boom".into(),
            source: None,
        }
        .is_retryable());
    }

    #[derive(Debug)]
    struct FakeDbError {
        message: String,
        code: Option<String>,
        constraint: bool,
    }

    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for FakeDbError {}

    impl sqlx::error::DatabaseError for FakeDbError {
        fn message(&self) -> &str {
            &self.message
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            self.code.as_deref().map(Into::into)
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            if self.constraint {
                sqlx::error::ErrorKind::UniqueViolation
            } else {
                sqlx::error::ErrorKind::Other
            }
        }
    }

    fn db_error(message: &str, code: Option<&str>, constraint: bool) -> StorageError {
        let fake = FakeDbError {
            message: message.into(),
            code: code.map(str::to_string),
            constraint,
        };
        sqlx::Error::Database(Box::new(fake)).into()
    }

    #[test]
    fn database_full_and_busy_classify() {
        assert!(db_error("database or disk is full", Some("13"), false).is_disk_full());
        for code in ["5", "6", "517"] {
            let err = db_error("database is locked", Some(code), false);
            assert!(matches!(err, StorageError::Pool { .. }), "{code}: {err:?}");
            assert!(err.is_retryable());
        }
    }

    #[test]
    fn database_constraints_are_caller_faults() {
        let err = db_error("UNIQUE constraint failed", Some("2067"), true);
        assert!(matches!(err, StorageError::InvalidQuery(_)), "{err:?}");
        let err = db_error("NOT NULL constraint failed", Some("19"), false);
        assert!(matches!(err, StorageError::InvalidQuery(_)), "{err:?}");
    }

    #[test]
    fn postgres_states_classify_by_class() {
        let err = db_error("could not serialize access", Some("40001"), false);
        assert!(matches!(err, StorageError::Pool { .. }), "{err:?}");
        let err = db_error("connection failure", Some("08006"), false);
        assert!(matches!(err, StorageError::Pool { .. }), "{err:?}");
        let err = db_error("duplicate key value", Some("23505"), false);
        assert!(matches!(err, StorageError::InvalidQuery(_)), "{err:?}");
        let err = db_error("syntax error", Some("42601"), false);
        assert!(matches!(err, StorageError::InvalidQuery(_)), "{err:?}");
        let err = db_error("password authentication failed", Some("28P01"), false);
        assert!(
            matches!(err, StorageError::Initialization { .. }),
            "{err:?}"
        );
        let err = db_error("internal error", Some("XX000"), false);
        assert!(matches!(err, StorageError::General { .. }), "{err:?}");
    }

    #[test]
    fn sqlite_numeric_and_sqlstate_namespaces_stay_disjoint() {
        assert!(sqlite_primary("13") == Some(13));
        assert!(sqlite_primary("2067") == Some(19));
        assert!(sqlite_primary("23505").is_none());
        assert!(is_sql_state("23505"));
        assert!(!is_sql_state("13"));
    }
}
