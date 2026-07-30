use crate::registry::RegistryError;

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("event bus error: {0}")]
    Event(#[from] EventError),
    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),
    #[error("invalid state transition: {message}")]
    InvalidStateTransition { message: String },
    #[error("snapshot error: {message}")]
    Snapshot { message: String },
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("condition error: {0}")]
    ConditionError(String),
    #[error("hook error: {0}")]
    HookError(String),
    #[error("interruption error: {0}")]
    InterruptionError(String),
    #[error("state error: {0}")]
    StateError(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(thiserror::Error, Debug)]
pub enum EventError {
    #[error("channel closed")]
    ChannelClosed,
    #[error("lagging behind by {0} messages")]
    Lagged(u64),
    #[error("capacity exceeded")]
    CapacityExceeded,
    #[error("send error: {0}")]
    Send(String),
    #[error("no events available")]
    Empty,
}

impl From<tokio::sync::broadcast::error::SendError<wf_types::events::BaseEvent>> for EventError {
    fn from(e: tokio::sync::broadcast::error::SendError<wf_types::events::BaseEvent>) -> Self {
        EventError::Send(e.to_string())
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for EventError {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        match e {
            tokio::sync::broadcast::error::RecvError::Closed => EventError::ChannelClosed,
            tokio::sync::broadcast::error::RecvError::Lagged(n) => EventError::Lagged(n),
        }
    }
}

impl From<tokio::sync::broadcast::error::TryRecvError> for EventError {
    fn from(e: tokio::sync::broadcast::error::TryRecvError) -> Self {
        match e {
            tokio::sync::broadcast::error::TryRecvError::Closed => EventError::ChannelClosed,
            tokio::sync::broadcast::error::TryRecvError::Lagged(n) => EventError::Lagged(n),
            tokio::sync::broadcast::error::TryRecvError::Empty => EventError::Empty,
        }
    }
}
