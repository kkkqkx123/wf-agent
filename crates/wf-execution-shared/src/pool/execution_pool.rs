use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::ExecutionSharedError;

pub struct ExecutionPool {
    semaphore: Arc<tokio::sync::Semaphore>,
    active_count: AtomicUsize,
}

impl ExecutionPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            active_count: AtomicUsize::new(0),
        }
    }

    pub async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>, ExecutionSharedError> {
        let permit = self.semaphore.acquire().await.map_err(|e| {
            ExecutionSharedError::PoolError(format!("Failed to acquire permit: {}", e))
        })?;
        self.active_count.fetch_add(1, Ordering::SeqCst);
        Ok(permit)
    }

    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}
