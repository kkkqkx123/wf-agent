use std::sync::Arc;

use tokio::sync::watch;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptionSignal {
    Active,
    Pause,
    Stop,
}

#[derive(Clone)]
pub struct InterruptionState {
    tx: watch::Sender<InterruptionSignal>,
    rx: watch::Receiver<InterruptionSignal>,
    parent: Option<Arc<InterruptionState>>,
}

impl InterruptionState {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(InterruptionSignal::Active);
        Self {
            tx,
            rx,
            parent: None,
        }
    }

    pub fn pause(&self) -> ExecutionSharedResult<()> {
        self.tx.send(InterruptionSignal::Pause).map_err(|_| {
            ExecutionSharedError::InterruptionError("failed to send pause signal".to_string())
        })
    }

    pub fn stop(&self) -> ExecutionSharedResult<()> {
        self.tx.send(InterruptionSignal::Stop).map_err(|_| {
            ExecutionSharedError::InterruptionError("failed to send stop signal".to_string())
        })
    }

    pub fn check(&self) -> Option<InterruptionSignal> {
        let current = self.rx.borrow().clone();
        match current {
            InterruptionSignal::Active => {
                if let Some(parent) = &self.parent {
                    parent.check()
                } else {
                    None
                }
            }
            signal => Some(signal),
        }
    }

    pub fn is_interrupted(&self) -> bool {
        self.check().is_some()
    }

    pub fn connect_to_parent(&mut self, parent: Arc<InterruptionState>) {
        self.parent = Some(parent);
    }

    pub fn parent(&self) -> Option<&Arc<InterruptionState>> {
        self.parent.as_ref()
    }

    pub fn subscribe(&self) -> watch::Receiver<InterruptionSignal> {
        self.rx.clone()
    }
}

impl Default for InterruptionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state_is_not_interrupted() {
        let state = InterruptionState::new();
        assert_eq!(state.check(), None);
        assert!(!state.is_interrupted());
    }

    #[test]
    fn test_pause_signal() {
        let state = InterruptionState::new();
        state.pause().unwrap();
        assert_eq!(state.check(), Some(InterruptionSignal::Pause));
        assert!(state.is_interrupted());
    }

    #[test]
    fn test_stop_signal() {
        let state = InterruptionState::new();
        state.stop().unwrap();
        assert_eq!(state.check(), Some(InterruptionSignal::Stop));
    }

    #[test]
    fn test_cascading_interruption() {
        let parent = InterruptionState::new();
        let mut child = InterruptionState::new();
        child.connect_to_parent(Arc::new(parent.clone()));

        assert_eq!(child.check(), None);

        parent.pause().unwrap();
        assert_eq!(child.check(), Some(InterruptionSignal::Pause));
    }

    #[test]
    fn test_child_interrupt_takes_priority() {
        let parent = InterruptionState::new();
        let mut child = InterruptionState::new();
        child.connect_to_parent(Arc::new(parent.clone()));

        child.stop().unwrap();
        assert_eq!(child.check(), Some(InterruptionSignal::Stop));

        parent.pause().unwrap();
        assert_eq!(child.check(), Some(InterruptionSignal::Stop));
    }

    #[test]
    fn test_subscribe_receives_signal() {
        let state = InterruptionState::new();
        let mut rx = state.subscribe();

        assert!(rx.borrow().clone() == InterruptionSignal::Active);

        state.pause().unwrap();
        rx.borrow_and_update();
        assert_eq!(rx.borrow().clone(), InterruptionSignal::Pause);
    }
}
