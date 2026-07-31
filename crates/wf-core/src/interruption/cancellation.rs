use tokio_util::sync::CancellationToken;

pub fn combine_cancellation_tokens(
    a: &CancellationToken,
    b: &CancellationToken,
) -> CancellationToken {
    let combined = CancellationToken::new();

    let a_clone = a.clone();
    let b_clone = b.clone();
    let combined_clone = combined.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = a_clone.cancelled() => combined_clone.cancel(),
            _ = b_clone.cancelled() => combined_clone.cancel(),
        }
    });

    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_combine_one_cancelled() {
        let a = CancellationToken::new();
        let b = CancellationToken::new();
        let combined = combine_cancellation_tokens(&a, &b);

        assert!(!combined.is_cancelled());
        a.cancel();

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(combined.is_cancelled());
    }

    #[tokio::test]
    async fn test_combine_other_cancelled() {
        let a = CancellationToken::new();
        let b = CancellationToken::new();
        let combined = combine_cancellation_tokens(&a, &b);

        assert!(!combined.is_cancelled());
        b.cancel();

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(combined.is_cancelled());
    }

    #[tokio::test]
    async fn test_combine_both_not_cancelled() {
        let a = CancellationToken::new();
        let b = CancellationToken::new();
        let combined = combine_cancellation_tokens(&a, &b);

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!combined.is_cancelled());
    }
}
