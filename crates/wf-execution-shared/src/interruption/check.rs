use super::state::{InterruptionSignal, InterruptionState};
use crate::types::interruption::ExecutionInterruptionCheckResult;

pub fn check_execution_interruption(
    state: &InterruptionState,
    current_iteration: Option<u32>,
) -> ExecutionInterruptionCheckResult {
    match state.check() {
        None => ExecutionInterruptionCheckResult::Continue,
        Some(InterruptionSignal::Pause) => ExecutionInterruptionCheckResult::Paused {
            iteration: current_iteration,
        },
        Some(InterruptionSignal::Stop) => ExecutionInterruptionCheckResult::Stopped {
            iteration: current_iteration,
        },
        Some(InterruptionSignal::Active) => ExecutionInterruptionCheckResult::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_continue_when_not_interrupted() {
        let state = InterruptionState::new();
        let result = check_execution_interruption(&state, Some(5));
        assert!(matches!(result, ExecutionInterruptionCheckResult::Continue));
    }

    #[test]
    fn test_paused() {
        let state = InterruptionState::new();
        state.pause().unwrap();
        let result = check_execution_interruption(&state, Some(3));
        assert!(matches!(
            result,
            ExecutionInterruptionCheckResult::Paused {
                iteration: Some(3)
            }
        ));
    }

    #[test]
    fn test_stopped() {
        let state = InterruptionState::new();
        state.stop().unwrap();
        let result = check_execution_interruption(&state, Some(7));
        assert!(matches!(
            result,
            ExecutionInterruptionCheckResult::Stopped {
                iteration: Some(7)
            }
        ));
    }

    #[test]
    fn test_iteration_none() {
        let state = InterruptionState::new();
        state.pause().unwrap();
        let result = check_execution_interruption(&state, None);
        assert!(matches!(
            result,
            ExecutionInterruptionCheckResult::Paused { iteration: None }
        ));
    }
}
