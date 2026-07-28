use super::state::{InterruptionSignal, InterruptionState};
use crate::error::ExecutionSharedResult;
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

pub async fn iterate_with_interruption_handling<F, Fut, T>(
    state: &InterruptionState,
    iteration: u32,
    f: F,
) -> ExecutionSharedResult<Option<T>>
where
    F: FnOnce(u32) -> Fut,
    Fut: std::future::Future<Output = ExecutionSharedResult<T>>,
{
    if state.is_cancelled() {
        return Ok(None);
    }

    let result = tokio::select! {
        result = f(iteration) => result.map(Some),
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
        } => Ok(None),
    }?;

    if state.is_cancelled() {
        return Ok(None);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

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

    #[tokio::test]
    async fn test_iterate_with_interruption_handling_completes() {
        let state = InterruptionState::new();
        let result = iterate_with_interruption_handling(&state, 0, |i| async move {
            Ok(i * 2)
        })
        .await
        .unwrap();
        assert_eq!(result, Some(0));

        let result = iterate_with_interruption_handling(&state, 5, |i| async move {
            Ok(i * 2)
        })
        .await
        .unwrap();
        assert_eq!(result, Some(10));
    }

    #[tokio::test]
    async fn test_iterate_cancelled_before() {
        let state = InterruptionState::new();
        state.stop().unwrap();
        let result = iterate_with_interruption_handling(&state, 0, |_| async move {
            Ok(42)
        })
        .await
        .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_iterate_cancelled_during() {
        let state = Arc::new(InterruptionState::new());
        let state_clone = Arc::clone(&state);

        let handle = tokio::spawn(async move {
            iterate_with_interruption_handling(&state_clone, 0, |_| async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(42)
            })
            .await
            .unwrap()
        });

        tokio::task::yield_now().await;
        state.stop().unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_iterate_error_propagation() {
        let state = InterruptionState::new();
        let result: ExecutionSharedResult<Option<i32>> = iterate_with_interruption_handling(&state, 0, |_| async move {
            Err(crate::error::ExecutionSharedError::Internal("fail".to_string()))
        })
        .await;
        assert!(result.is_err());
    }
}
