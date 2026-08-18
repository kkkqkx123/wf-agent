use std::future::Future;

/// Runs `future` with an optional timeout in milliseconds.
///
/// All execution strategies must route their command execution through this
/// helper so the `timeout_limit_ms` policy is uniformly enforced. On timeout
/// an error describing the deadline is returned.
pub async fn execute_with_timeout<F, T, E>(
    future: F,
    timeout_ms: Option<u64>,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let fut = async move { future.await.map_err(Into::into) };
    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(res) => res,
            Err(_) => Err(format!("Script execution timed out after {ms}ms").into()),
        },
        None => fut.await,
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
        let result: Result<u32, Box<dyn std::error::Error + Send + Sync>> = execute_with_timeout(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok::<_, std::io::Error>(1u32)
            },
            Some(50),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_timeout_not_triggered_when_fast() {
        let result = execute_with_timeout(async { Ok::<_, std::io::Error>(7u32) }, Some(1000))
            .await
            .unwrap();
        assert_eq!(result, 7);
    }
}
