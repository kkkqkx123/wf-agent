use std::time::Duration;

use wf_core::interruption::{InterruptionSignal, InterruptionState};
use wf_execution_shared::{
    is_pause_signal, is_paused, is_stop_signal, is_stopped, wait_for_resume, HasInterruption,
    LoopDecision,
};

struct Probe {
    state: InterruptionState,
}

impl HasInterruption for Probe {
    fn interruption(&self) -> &InterruptionState {
        &self.state
    }
}

fn probe() -> Probe {
    Probe {
        state: InterruptionState::new(),
    }
}

#[test]
fn loop_decision_from_none_and_active_is_continue() {
    assert_eq!(LoopDecision::from(None), LoopDecision::Continue);
    assert_eq!(
        LoopDecision::from(Some(InterruptionSignal::Active)),
        LoopDecision::Continue
    );
}

#[test]
fn loop_decision_from_pause_and_stop() {
    assert_eq!(
        LoopDecision::from(Some(InterruptionSignal::Pause)),
        LoopDecision::Pause
    );
    assert_eq!(
        LoopDecision::from(Some(InterruptionSignal::Stop)),
        LoopDecision::Stop
    );
}

#[test]
fn signal_predicates_match_variants() {
    assert!(is_stop_signal(Some(InterruptionSignal::Stop)));
    assert!(!is_stop_signal(Some(InterruptionSignal::Pause)));
    assert!(!is_stop_signal(None));
    assert!(is_pause_signal(Some(InterruptionSignal::Pause)));
    assert!(!is_pause_signal(Some(InterruptionSignal::Stop)));
    assert!(!is_pause_signal(None));
}

#[test]
fn entity_predicates_reflect_current_signal() {
    let entity = probe();
    assert!(!is_stopped(&entity));
    assert!(!is_paused(&entity));

    entity.interruption().pause().expect("pause succeeds");
    assert!(is_paused(&entity));
    assert!(!is_stopped(&entity));

    entity.interruption().stop().expect("stop succeeds");
    assert!(is_stopped(&entity));
}

#[tokio::test]
async fn wait_for_resume_returns_immediately_when_active() {
    let entity = probe();
    tokio::time::timeout(
        Duration::from_millis(50),
        wait_for_resume(entity.interruption()),
    )
    .await
    .expect("active state returns without waiting");
}

#[tokio::test]
async fn wait_for_resume_returns_after_resume() {
    let entity = probe();
    entity.interruption().pause().expect("pause succeeds");
    let waiter = tokio::spawn({
        let state = entity.interruption().clone();
        async move { wait_for_resume(&state).await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    entity.interruption().resume().expect("resume succeeds");
    tokio::time::timeout(Duration::from_millis(200), waiter)
        .await
        .expect("resume wakes waiter")
        .expect("waiter joins");
}

#[tokio::test]
async fn wait_for_resume_returns_on_stop() {
    let entity = probe();
    entity.interruption().pause().expect("pause succeeds");
    let waiter = tokio::spawn({
        let state = entity.interruption().clone();
        async move { wait_for_resume(&state).await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    entity.interruption().stop().expect("stop succeeds");
    tokio::time::timeout(Duration::from_millis(200), waiter)
        .await
        .expect("stop wakes waiter")
        .expect("waiter joins");
}
