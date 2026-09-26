use wf_core::interruption::{InterruptionSignal, InterruptionState};

use super::check::check_execution_interruption;
use crate::error::{ExecutionSharedError, ExecutionSharedResult, InterruptionKind};
use crate::types::interruption::ExecutionInterruptionCheckResult;

/// Boundary conversion of control-flow facts into typed errors. The sibling
/// `check` module reports cancellation as an in-loop exit signal (`Ok(None)`)
/// because a cancelled iteration is not an error to its own loop; this
/// function is the single place that turns the same facts into errors when
/// they cross a handler boundary, always tagged with the interruption kind
/// so downstream routing reads the type instead of the message.
pub async fn execute_with_interruption_handling<T, F, Fut>(
    state: &InterruptionState,
    current_iteration: Option<u32>,
    f: F,
) -> ExecutionSharedResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ExecutionSharedResult<T>>,
{
    match check_execution_interruption(state, current_iteration) {
        ExecutionInterruptionCheckResult::Continue => {}
        ExecutionInterruptionCheckResult::Paused { iteration } => {
            return Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Pause,
                detail: format!("execution paused at iteration {iteration:?}"),
            });
        }
        ExecutionInterruptionCheckResult::Stopped { iteration } => {
            return Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Stop,
                detail: format!("execution stopped at iteration {iteration:?}"),
            });
        }
        ExecutionInterruptionCheckResult::Aborted { reason } => {
            return Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Abort,
                detail: format!("execution aborted: {reason}"),
            });
        }
    }

    let result = tokio::select! {
        result = f() => result?,
        _ = async {
            let mut rx = state.subscribe();
            loop {
                if *rx.borrow() == InterruptionSignal::Stop {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        } => {
            return Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Stop,
                detail: format!("execution stopped at iteration {current_iteration:?}"),
            });
        }
    };

    match check_execution_interruption(state, current_iteration) {
        ExecutionInterruptionCheckResult::Continue => Ok(result),
        ExecutionInterruptionCheckResult::Paused { iteration } => {
            Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Pause,
                detail: format!("execution paused after operation at iteration {iteration:?}"),
            })
        }
        ExecutionInterruptionCheckResult::Stopped { iteration } => {
            Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Stop,
                detail: format!("execution stopped after operation at iteration {iteration:?}"),
            })
        }
        ExecutionInterruptionCheckResult::Aborted { reason } => {
            Err(ExecutionSharedError::InterruptionError {
                kind: InterruptionKind::Abort,
                detail: format!("execution aborted after operation: {reason}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::interruption::InterruptionState;

    #[tokio::test]
    async fn test_execute_when_not_interrupted() {
        let state = InterruptionState::new();
        let result = execute_with_interruption_handling(&state, Some(0), || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_execute_rejects_when_paused_before() {
        let state = InterruptionState::new();
        state.pause().unwrap();
        let result: ExecutionSharedResult<i32> =
            execute_with_interruption_handling(&state, Some(0), || async { Ok(42) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_rejects_when_stopped_before() {
        let state = InterruptionState::new();
        state.stop().unwrap();
        let result: ExecutionSharedResult<i32> =
            execute_with_interruption_handling(&state, Some(0), || async { Ok(42) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_detects_interruption_after_operation() {
        let state = InterruptionState::new();
        let state_clone = state.clone();
        let result: ExecutionSharedResult<i32> =
            execute_with_interruption_handling(&state, Some(0), || async {
                state_clone.pause().unwrap();
                Ok(42)
            })
            .await;
        assert!(result.is_err());
    }
}
