#[derive(Debug, Clone)]
pub struct CommonError {
    pub kind: CommonErrorKind,
    pub message: String,
    pub source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommonErrorKind {
    InvalidArgument,
    NotFound,
    AlreadyExists,
    Timeout,
    Internal,
    Serialization,
    Io,
}

impl fmt::Display for CommonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for CommonError {}

impl CommonError {
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::InvalidArgument,
            message: msg.into(),
            source: None,
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::NotFound,
            message: msg.into(),
            source: None,
        }
    }

    pub fn already_exists(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::AlreadyExists,
            message: msg.into(),
            source: None,
        }
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::Timeout,
            message: msg.into(),
            source: None,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::Internal,
            message: msg.into(),
            source: None,
        }
    }

    pub fn serialization(msg: impl Into<String>) -> Self {
        Self {
            kind: CommonErrorKind::Serialization,
            message: msg.into(),
            source: None,
        }
    }
}
