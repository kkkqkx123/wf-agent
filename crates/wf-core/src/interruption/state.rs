use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{watch, Notify};
use tokio_util::sync::CancellationToken;
use wf_types::events::{BaseEvent, EventType};

use crate::error::{CoreError, CoreResult};
use crate::event::EventBus;

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
    cancellation_token: CancellationToken,
    event_bus: Option<Arc<EventBus>>,
    resume_notify: Arc<Notify>,
    disposed: Arc<AtomicBool>,
}

impl InterruptionState {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(InterruptionSignal::Active);
        Self {
            tx,
            rx,
            parent: None,
            cancellation_token: CancellationToken::new(),
            event_bus: None,
            resume_notify: Arc::new(Notify::new()),
            disposed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn pause(&self) -> CoreResult<()> {
        self.ensure_not_disposed()?;
        self.tx
            .send(InterruptionSignal::Pause)
            .map_err(|_| CoreError::InterruptionError("failed to send pause signal".to_string()))?;
        self.cancellation_token.cancel();
        self.emit_event(EventType::ExecutionPaused);
        Ok(())
    }

    pub fn stop(&self) -> CoreResult<()> {
        self.ensure_not_disposed()?;
        self.tx
            .send(InterruptionSignal::Stop)
            .map_err(|_| CoreError::InterruptionError("failed to send stop signal".to_string()))?;
        self.cancellation_token.cancel();
        self.emit_event(EventType::ExecutionCancelled);
        Ok(())
    }

    pub fn resume(&self) -> CoreResult<()> {
        self.ensure_not_disposed()?;
        self.tx.send(InterruptionSignal::Active).map_err(|_| {
            CoreError::InterruptionError("failed to send resume signal".to_string())
        })?;
        self.resume_notify.notify_waiters();
        self.emit_event(EventType::ExecutionResumed);
        Ok(())
    }

    pub async fn on_resumed(&self) {
        self.resume_notify.notified().await;
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::SeqCst);
        self.cancellation_token.cancel();
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
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

    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub fn child_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    fn ensure_not_disposed(&self) -> CoreResult<()> {
        if self.is_disposed() {
            Err(CoreError::InterruptionError(
                "interruption state has been disposed".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn emit_event(&self, event_type: EventType) {
        if let Some(bus) = &self.event_bus {
            let event = BaseEvent {
                id: wf_types::Id::new(),
                r#type: event_type,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                metadata: None,
            };
            let _ = bus.publish(event);
        }
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
        assert!(state.is_cancelled());
    }

    #[test]
    fn test_stop_signal() {
        let state = InterruptionState::new();
        state.stop().unwrap();
        assert_eq!(state.check(), Some(InterruptionSignal::Stop));
        assert!(state.is_cancelled());
    }

    #[test]
    fn test_resume() {
        let state = InterruptionState::new();
        state.pause().unwrap();
        assert!(state.is_interrupted());
        state.resume().unwrap();
        assert!(!state.is_interrupted());
        assert_eq!(state.check(), None);
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

    #[test]
    fn test_cancellation_token_propagation() {
        let state = InterruptionState::new();
        let token = state.get_cancellation_token();
        assert!(!token.is_cancelled());

        state.pause().unwrap();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_child_token_cascade() {
        let parent = InterruptionState::new();
        let child_token = parent.child_token();
        assert!(!child_token.is_cancelled());

        parent.stop().unwrap();
        assert!(child_token.is_cancelled());
    }

    #[test]
    fn test_resume_keeps_old_token_cancelled() {
        let state = InterruptionState::new();
        let token = state.get_cancellation_token();

        state.pause().unwrap();
        assert!(token.is_cancelled());

        state.resume().unwrap();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_disposed_prevents_operations() {
        let state = InterruptionState::new();
        state.dispose();
        assert!(state.is_disposed());
        assert!(state.pause().is_err());
        assert!(state.stop().is_err());
        assert!(state.resume().is_err());
    }

    #[test]
    fn test_disposed_cancels_token() {
        let state = InterruptionState::new();
        assert!(!state.is_cancelled());
        state.dispose();
        assert!(state.is_cancelled());
    }

    #[tokio::test]
    async fn test_on_resumed_notifies() {
        let state = InterruptionState::new();
        state.pause().unwrap();

        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            state_clone.on_resumed().await;
        });

        tokio::task::yield_now().await;
        state.resume().unwrap();

        handle.await.unwrap();
    }

    #[test]
    fn test_event_bus_emits_on_pause() {
        let bus = Arc::new(EventBus::new(16));
        let mut state = InterruptionState::new();
        state.set_event_bus(bus.clone());

        let mut sub = bus.subscribe();

        state.pause().unwrap();

        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::ExecutionPaused);
    }

    #[test]
    fn test_event_bus_emits_on_resume() {
        let bus = Arc::new(EventBus::new(16));
        let mut state = InterruptionState::new();
        state.set_event_bus(bus.clone());

        state.pause().unwrap();

        let mut sub = bus.subscribe();

        state.resume().unwrap();

        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::ExecutionResumed);
    }

    #[test]
    fn test_event_bus_emits_on_stop() {
        let bus = Arc::new(EventBus::new(16));
        let mut state = InterruptionState::new();
        state.set_event_bus(bus.clone());

        let mut sub = bus.subscribe();

        state.stop().unwrap();

        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::ExecutionCancelled);
    }
}
