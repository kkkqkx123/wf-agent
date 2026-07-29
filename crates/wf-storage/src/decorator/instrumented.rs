use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;

use crate::domain::store::{BatchItem, BatchStore, Maintainable, QueryFilter, Store};
use crate::error::StorageError;

#[derive(Default)]
pub struct StorageMetrics {
    pub save: OperationMetrics,
    pub load: OperationMetrics,
    pub delete: OperationMetrics,
    pub list: OperationMetrics,
    pub exists: OperationMetrics,
    pub clear: OperationMetrics,
    pub batch: OperationMetrics,
}

#[derive(Default)]
pub struct OperationMetrics {
    pub count: std::sync::atomic::AtomicU64,
    pub total_time_ms: std::sync::atomic::AtomicU64,
    pub total_bytes: std::sync::atomic::AtomicU64,
}

impl OperationMetrics {
    pub fn avg_time_ms(&self) -> f64 {
        let c = self.count.load(Ordering::Relaxed);
        if c == 0 {
            0.0
        } else {
            self.total_time_ms.load(Ordering::Relaxed) as f64 / c as f64
        }
    }

    pub fn avg_bytes(&self) -> f64 {
        let c = self.count.load(Ordering::Relaxed);
        if c == 0 {
            0.0
        } else {
            self.total_bytes.load(Ordering::Relaxed) as f64 / c as f64
        }
    }

    pub fn record(&self, elapsed_ms: u64, bytes: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_time_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }
}

pub struct InstrumentedStore<S> {
    inner: S,
    metrics: Arc<StorageMetrics>,
}

impl<S: Store> InstrumentedStore<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            metrics: Arc::new(StorageMetrics::default()),
        }
    }

    pub fn metrics(&self) -> &StorageMetrics {
        &self.metrics
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

#[async_trait]
impl<S: Store> Store for InstrumentedStore<S> {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.save(id, data, metadata).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.save.record(elapsed, data.len() as u64);
        result
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let start = Instant::now();
        let result = self.inner.load(id).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let bytes = match &result {
            Ok(Some((d, _))) => d.len() as u64,
            _ => 0,
        };
        self.metrics.load.record(elapsed, bytes);
        result
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.delete(id).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.delete.record(elapsed, 0);
        result
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        let start = Instant::now();
        let result = self.inner.list(filter).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.list.record(elapsed, 0);
        result
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let start = Instant::now();
        let result = self.inner.exists(id).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.exists.record(elapsed, 0);
        result
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.clear().await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.clear.record(elapsed, 0);
        result
    }
}

impl<S: Store + BatchStore> BatchStore for InstrumentedStore<S> {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.save_batch(items).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let total_bytes: u64 = items.iter().map(|i| i.data.len() as u64).sum();
        self.metrics.batch.record(elapsed, total_bytes);
        result
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        let start = Instant::now();
        let result = self.inner.load_batch(ids).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let bytes = match &result {
            Ok(items) => items.iter().map(|(_, d, _)| d.len() as u64).sum(),
            Err(_) => 0,
        };
        self.metrics.load.record(elapsed, bytes);
        result
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.delete_batch(ids).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.delete.count.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .delete
            .total_time_ms
            .fetch_add(elapsed, Ordering::Relaxed);
        result
    }
}

impl<S: Store + Maintainable> Maintainable for InstrumentedStore<S> {
    async fn vacuum(&self) -> Result<(), StorageError> {
        self.inner.vacuum().await
    }

    async fn checkpoint(&self) -> Result<(), StorageError> {
        self.inner.checkpoint().await
    }

    async fn sync(&self) -> Result<(), StorageError> {
        self.inner.sync().await
    }
}
