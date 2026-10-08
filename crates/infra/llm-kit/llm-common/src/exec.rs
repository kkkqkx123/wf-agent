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

/// Runs `future` with an optional timeout in milliseconds.
///
/// When `timeout_ms` is `None` the future runs without a deadline.
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
