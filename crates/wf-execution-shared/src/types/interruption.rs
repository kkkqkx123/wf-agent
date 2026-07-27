use wf_types::Id;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InterruptionType {
    Pause,
    Stop,
}

#[derive(Debug, Clone, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("Interruption: type={interruption_type:?}, execution_id={execution_id}, iteration={iteration:?}")]
pub struct InterruptedException {
    pub interruption_type: InterruptionType,
    pub execution_id: Id,
    pub iteration: Option<u32>,
}

#[derive(Debug, Clone, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("Abort: {reason}")]
pub struct AbortError {
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExecutionInterruptionCheckResult {
    Continue,
    Paused { iteration: Option<u32> },
    Stopped { iteration: Option<u32> },
    Aborted { reason: String },
}
