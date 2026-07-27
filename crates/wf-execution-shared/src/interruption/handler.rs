use super::check::check_execution_interruption;
use super::state::InterruptionState;
use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::types::interruption::ExecutionInterruptionCheckResult;

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
            return Err(ExecutionSharedError::InterruptionError(format!(
                "execution paused at iteration {:?}",
                iteration
            )));
        }
        ExecutionInterruptionCheckResult::Stopped { iteration } => {
            return Err(ExecutionSharedError::InterruptionError(format!(
                "execution stopped at iteration {:?}",
                iteration
            )));
        }
        ExecutionInterruptionCheckResult::Aborted { reason } => {
            return Err(ExecutionSharedError::InterruptionError(format!(
                "execution aborted: {}",
                reason
            )));
        }
    }

    let result = f().await?;

    match check_execution_interruption(state, current_iteration) {
        ExecutionInterruptionCheckResult::Continue => Ok(result),
        ExecutionInterruptionCheckResult::Paused { iteration } => {
            Err(ExecutionSharedError::InterruptionError(format!(
                "execution paused after operation at iteration {:?}",
                iteration
            )))
        }
        ExecutionInterruptionCheckResult::Stopped { iteration } => {
            Err(ExecutionSharedError::InterruptionError(format!(
                "execution stopped after operation at iteration {:?}",
                iteration
            )))
        }
        ExecutionInterruptionCheckResult::Aborted { reason } => {
            Err(ExecutionSharedError::InterruptionError(format!(
                "execution aborted after operation: {}",
                reason
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_when_not_interrupted() {
        let state = InterruptionState::new();
        let result = execute_with_interruption_handling(&state, Some(0), || async {
            Ok(42)
        })
        .await;
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
