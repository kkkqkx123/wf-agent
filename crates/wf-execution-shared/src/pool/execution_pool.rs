use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::error::ExecutionSharedError;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PoolStats {
    pub max_concurrent: usize,
    pub active_count: usize,
    pub available_permits: usize,
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_cancelled: u64,
    pub total_timed_out: u64,
}

pub struct ExecutionPool {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    active_count: Arc<AtomicUsize>,
    cancel_token: CancellationToken,
    total_submitted: Arc<AtomicU64>,
    total_completed: Arc<AtomicU64>,
    total_cancelled: Arc<AtomicU64>,
    total_timed_out: Arc<AtomicU64>,
}

pub struct PoolPermit {
    _permit: OwnedSemaphorePermit,
    active_count: Arc<AtomicUsize>,
}

impl Drop for PoolPermit {
    fn drop(&mut self) {
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ExecutionPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            active_count: Arc::new(AtomicUsize::new(0)),
            cancel_token: CancellationToken::new(),
            total_submitted: Arc::new(AtomicU64::new(0)),
            total_completed: Arc::new(AtomicU64::new(0)),
            total_cancelled: Arc::new(AtomicU64::new(0)),
            total_timed_out: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn acquire(&self) -> Result<PoolPermit, ExecutionSharedError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| {
                ExecutionSharedError::PoolError(format!("failed to acquire permit: {}", e))
            })?;

        self.active_count.fetch_add(1, Ordering::SeqCst);

        Ok(PoolPermit {
            _permit: permit,
            active_count: Arc::clone(&self.active_count),
        })
    }

    pub async fn submit<F, T>(&self, f: F) -> Result<T, ExecutionSharedError>
    where
        F: std::future::Future<Output = Result<T, ExecutionSharedError>> + Send + 'static,
        T: Send + 'static,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = self.acquire().await?;
        let result = f.await;
        if result.is_ok() {
            self.total_completed.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub async fn submit_with_timeout<F, T>(
        &self,
        duration: Duration,
        f: F,
    ) -> Result<T, ExecutionSharedError>
    where
        F: std::future::Future<Output = Result<T, ExecutionSharedError>> + Send + 'static,
        T: Send + 'static,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = self.acquire().await?;
        let result = tokio::time::timeout(duration, f).await;
        match result {
            Ok(Ok(val)) => {
                self.total_completed.fetch_add(1, Ordering::Relaxed);
                Ok(val)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.total_timed_out.fetch_add(1, Ordering::Relaxed);
                Err(ExecutionSharedError::TimeoutError(format!(
                    "task timed out after {:?}",
                    duration
                )))
            }
        }
    }

    pub async fn submit_with_cancel<F, T>(
        &self,
        external_token: &CancellationToken,
        f: F,
    ) -> Result<T, ExecutionSharedError>
    where
        F: std::future::Future<Output = Result<T, ExecutionSharedError>> + Send + 'static,
        T: Send + 'static,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = self.acquire().await?;
        if external_token.is_cancelled() {
            self.total_cancelled.fetch_add(1, Ordering::Relaxed);
            return Err(ExecutionSharedError::InterruptionError(
                "task cancelled by external token".to_string(),
            ));
        }
        if self.cancel_token.is_cancelled() {
            self.total_cancelled.fetch_add(1, Ordering::Relaxed);
            return Err(ExecutionSharedError::InterruptionError(
                "task cancelled by pool shutdown".to_string(),
            ));
        }
        let pool_token = self.cancel_token.child_token();
        tokio::select! {
            result = f => {
                if result.is_ok() {
                    self.total_completed.fetch_add(1, Ordering::Relaxed);
                }
                result
            },
            _ = external_token.cancelled() => {
                self.total_cancelled.fetch_add(1, Ordering::Relaxed);
                Err(ExecutionSharedError::InterruptionError(
                    "task cancelled by external token".to_string()
                ))
            },
            _ = pool_token.cancelled() => {
                self.total_cancelled.fetch_add(1, Ordering::Relaxed);
                Err(ExecutionSharedError::InterruptionError(
                    "task cancelled by pool shutdown".to_string()
                ))
            },
        }
    }

    pub async fn submit_with_timeout_and_cancel<F, T>(
        &self,
        duration: Duration,
        external_token: &CancellationToken,
        f: F,
    ) -> Result<T, ExecutionSharedError>
    where
        F: std::future::Future<Output = Result<T, ExecutionSharedError>> + Send + 'static,
        T: Send + 'static,
    {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        let _permit = self.acquire().await?;
        if external_token.is_cancelled() {
            self.total_cancelled.fetch_add(1, Ordering::Relaxed);
            return Err(ExecutionSharedError::InterruptionError(
                "task cancelled by external token".to_string(),
            ));
        }
        if self.cancel_token.is_cancelled() {
            self.total_cancelled.fetch_add(1, Ordering::Relaxed);
            return Err(ExecutionSharedError::InterruptionError(
                "task cancelled by pool shutdown".to_string(),
            ));
        }
        let pool_token = self.cancel_token.child_token();
        let timeout_future = tokio::time::timeout(duration, f);
        tokio::select! {
            result = timeout_future => match result {
                Ok(Ok(val)) => {
                    self.total_completed.fetch_add(1, Ordering::Relaxed);
                    Ok(val)
                }
                Ok(Err(e)) => Err(e),
                Err(_) => {
                    self.total_timed_out.fetch_add(1, Ordering::Relaxed);
                    Err(ExecutionSharedError::TimeoutError(format!(
                        "task timed out after {:?}",
                        duration
                    )))
                }
            },
            _ = external_token.cancelled() => {
                self.total_cancelled.fetch_add(1, Ordering::Relaxed);
                Err(ExecutionSharedError::InterruptionError(
                    "task cancelled by external token".to_string()
                ))
            },
            _ = pool_token.cancelled() => {
                self.total_cancelled.fetch_add(1, Ordering::Relaxed);
                Err(ExecutionSharedError::InterruptionError(
                    "task cancelled by pool shutdown".to_string()
                ))
            },
        }
    }

    pub fn shutdown(&self) {
        self.semaphore.close();
        self.cancel_token.cancel();
    }

    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    pub fn is_shutdown(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub fn stats(&self) -> PoolStats {
        PoolStats {
            max_concurrent: self.max_concurrent,
            active_count: self.active_count.load(Ordering::Relaxed),
            available_permits: self.semaphore.available_permits(),
            total_submitted: self.total_submitted.load(Ordering::Relaxed),
            total_completed: self.total_completed.load(Ordering::Relaxed),
            total_cancelled: self.total_cancelled.load(Ordering::Relaxed),
            total_timed_out: self.total_timed_out.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release() {
        let pool = ExecutionPool::new(2);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.available_permits(), 2);

        let permit1 = pool.acquire().await.unwrap();
        assert_eq!(pool.active_count(), 1);
        assert_eq!(pool.available_permits(), 1);

        let permit2 = pool.acquire().await.unwrap();
        assert_eq!(pool.active_count(), 2);
        assert_eq!(pool.available_permits(), 0);

        drop(permit1);
        assert_eq!(pool.active_count(), 1);
        assert_eq!(pool.available_permits(), 1);

        drop(permit2);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_max_concurrent() {
        let pool = ExecutionPool::new(5);
        assert_eq!(pool.max_concurrent(), 5);
    }

    #[tokio::test]
    async fn test_submit() {
        let pool = ExecutionPool::new(2);
        let result = pool.submit(async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_submit_with_timeout_success() {
        let pool = ExecutionPool::new(2);
        let result = pool
            .submit_with_timeout(Duration::from_secs(5), async { Ok(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_submit_with_timeout_expired() {
        let pool = ExecutionPool::new(2);
        let result: Result<i32, ExecutionSharedError> = pool
            .submit_with_timeout(Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(42)
            })
            .await;
        assert!(matches!(result, Err(ExecutionSharedError::TimeoutError(_))));
    }

    #[tokio::test]
    async fn test_submit_error_propagation() {
        let pool = ExecutionPool::new(2);
        let result: Result<i32, ExecutionSharedError> = pool
            .submit(async {
                Err(ExecutionSharedError::Internal("task failed".to_string()))
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shutdown_prevents_new_acquisitions() {
        let pool = ExecutionPool::new(2);
        pool.shutdown();
        assert!(pool.acquire().await.is_err());
    }

    #[tokio::test]
    async fn test_shutdown_cancels_token() {
        let pool = ExecutionPool::new(2);
        assert!(!pool.is_shutdown());
        pool.shutdown();
        assert!(pool.is_shutdown());
    }

    #[tokio::test]
    async fn test_concurrent_submissions() {
        let pool = ExecutionPool::new(2);
        let mut handles = Vec::new();

        for i in 0..5 {
            handles.push(pool.submit(async move { Ok(i) }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        results.sort();
        assert_eq!(results, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_submit_with_cancel_external() {
        let pool = ExecutionPool::new(2);
        let external = CancellationToken::new();
        external.cancel();

        let result: Result<i32, ExecutionSharedError> = pool
            .submit_with_cancel(&external, async { Ok(42) })
            .await;
        assert!(matches!(
            result,
            Err(ExecutionSharedError::InterruptionError(_))
        ));
    }

    #[tokio::test]
    async fn test_submit_with_cancel_success() {
        let pool = ExecutionPool::new(2);
        let external = CancellationToken::new();

        let result = pool
            .submit_with_cancel(&external, async { Ok(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_submit_with_cancel_pool_shutdown() {
        let pool = Arc::new(ExecutionPool::new(2));
        let external = CancellationToken::new();

        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            pool_clone
                .submit_with_cancel(&external, async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    Ok(42)
                })
                .await
        });

        tokio::task::yield_now().await;
        pool.shutdown();

        let result = handle.await.unwrap();
        assert!(matches!(
            result,
            Err(ExecutionSharedError::InterruptionError(_))
        ));
    }

    #[tokio::test]
    async fn test_submit_with_timeout_and_cancel() {
        let pool = ExecutionPool::new(2);
        let external = CancellationToken::new();

        let result = pool
            .submit_with_timeout_and_cancel(Duration::from_secs(5), &external, async { Ok(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_submit_with_timeout_and_cancel_external_fires() {
        let pool = ExecutionPool::new(2);
        let external = CancellationToken::new();
        external.cancel();

        let result: Result<i32, ExecutionSharedError> = pool
            .submit_with_timeout_and_cancel(Duration::from_secs(5), &external, async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(42)
            })
            .await;
        assert!(matches!(
            result,
            Err(ExecutionSharedError::InterruptionError(_))
        ));
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = ExecutionPool::new(2);

        let stats = pool.stats();
        assert_eq!(stats.max_concurrent, 2);
        assert_eq!(stats.total_submitted, 0);
        assert_eq!(stats.total_completed, 0);

        pool.submit(async { Ok(1) }).await.unwrap();
        pool.submit(async { Ok(2) }).await.unwrap();

        let stats = pool.stats();
        assert_eq!(stats.total_submitted, 2);
        assert_eq!(stats.total_completed, 2);
    }

    #[tokio::test]
    async fn test_pool_stats_timeout() {
        let pool = ExecutionPool::new(2);

        let result: Result<i32, ExecutionSharedError> = pool
            .submit_with_timeout(Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(42)
            })
            .await;
        assert!(result.is_err());

        let stats = pool.stats();
        assert_eq!(stats.total_submitted, 1);
        assert_eq!(stats.total_timed_out, 1);
        assert_eq!(stats.total_completed, 0);
    }

    #[tokio::test]
    async fn test_pool_stats_cancelled() {
        let pool = ExecutionPool::new(2);
        let external = CancellationToken::new();
        external.cancel();

        let result: Result<i32, ExecutionSharedError> = pool
            .submit_with_cancel(&external, async { Ok(42) })
            .await;
        assert!(result.is_err());

        let stats = pool.stats();
        assert_eq!(stats.total_submitted, 1);
        assert_eq!(stats.total_cancelled, 1);
        assert_eq!(stats.total_completed, 0);
    }
}
