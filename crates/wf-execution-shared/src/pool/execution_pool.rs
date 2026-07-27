use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::ExecutionSharedError;

pub struct ExecutionPool {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    active_count: Arc<AtomicUsize>,
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

    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
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
}
