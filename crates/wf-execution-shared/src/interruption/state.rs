use tokio::sync::watch;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptionType {
    Pause,
    Stop,
}

pub struct InterruptionState {
    tx: watch::Sender<InterruptionType>,
    rx: watch::Receiver<InterruptionType>,
}

impl InterruptionState {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(InterruptionType::Pause);
        Self { tx, rx }
    }

    pub fn pause(&self) -> ExecutionSharedResult<()> {
        self.tx.send(InterruptionType::Pause).map_err(|_| {
            ExecutionSharedError::InterruptionError("Failed to send pause".to_string())
        })
    }

    pub fn stop(&self) -> ExecutionSharedResult<()> {
        self.tx.send(InterruptionType::Stop).map_err(|_| {
            ExecutionSharedError::InterruptionError("Failed to send stop".to_string())
        })
    }

    pub fn check(&self) -> Option<InterruptionType> {
        let current = self.rx.borrow().clone();
        match current {
            InterruptionType::Pause | InterruptionType::Stop => Some(current),
        }
    }
}
