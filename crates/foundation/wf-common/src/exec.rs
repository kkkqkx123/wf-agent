use std::future::Future;
use std::time::Duration;

/// Structured failure of a timeout-wrapped execution: either the deadline
/// expired or the underlying future returned an error.
#[derive(Debug)]
pub enum TimeoutError<E> {
    /// The deadline expired before the future completed; carries the
    /// configured timeout in milliseconds.
    TimedOut(u64),
    /// The future completed with an error.
    Failed(E),
}

impl<E> TimeoutError<E> {
    /// Convert into a boxed error, mapping the deadline expiry to a message.
    pub fn into_boxed(self) -> Box<dyn std::error::Error + Send + Sync>
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        match self {
            TimeoutError::TimedOut(ms) => format!("Execution timed out after {ms}ms").into(),
            TimeoutError::Failed(e) => e.into(),
        }
    }
}

/// Runs `future` with an optional timeout in milliseconds.
///
/// When `timeout_ms` is `None` the future runs without a deadline. All
/// execution strategies should route their command execution through this
/// helper so a uniform timeout policy is enforced.
pub async fn execute_with_timeout<F, T, E>(
    future: F,
    timeout_ms: Option<u64>,
) -> Result<T, TimeoutError<E>>
where
    F: Future<Output = Result<T, E>>,
{
    match timeout_ms {
        Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), future).await {
            Ok(res) => res.map_err(TimeoutError::Failed),
            Err(_) => Err(TimeoutError::TimedOut(ms)),
        },
        None => future.await.map_err(TimeoutError::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_timeout_returns_value() {
        let result = execute_with_timeout(async { Ok::<_, std::io::Error>(42u32) }, None)
            .await
            .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_timeout_triggers() {
        let result: Result<u32, TimeoutError<std::io::Error>> = execute_with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok::<_, std::io::Error>(1u32)
            },
            Some(20),
        )
        .await;
        assert!(matches!(result, Err(TimeoutError::TimedOut(20))));
    }

    #[tokio::test]
    async fn test_underlying_error_is_preserved() {
        let result: Result<u32, TimeoutError<std::io::Error>> = execute_with_timeout(
            async { Err::<u32, _>(std::io::Error::other("boom")) },
            Some(1000),
        )
        .await;
        assert!(matches!(result, Err(TimeoutError::Failed(_))));
    }
}
