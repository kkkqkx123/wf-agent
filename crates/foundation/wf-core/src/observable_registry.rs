use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::registry::{
    BatchRegistry, ConcurrentRegistry, MutableRegistry, Registry, RegistryResult,
};

/// Listens for registry state-change events.
///
/// Registered via [`ObservableRegistry::add_listener`] and called
/// synchronously on each register / unregister operation.
#[async_trait]
pub trait RegistryEventListener: Send + Sync {
    async fn on_registered(&self, key: &str);
    async fn on_unregistered(&self, key: &str);
}

/// A registry wrapper that notifies [`RegistryEventListener`]s on each
/// mutation.
///
/// Delegates all reads/writes to the inner [`ConcurrentRegistry`] and
/// implements [`Registry`], [`MutableRegistry`], and [`BatchRegistry`] so
/// it can be used as a drop-in replacement.
pub struct ObservableRegistry<T: Send + Sync> {
    inner: Arc<ConcurrentRegistry<T>>,
    listeners: Vec<Arc<dyn RegistryEventListener>>,
    notify_error_count: Arc<AtomicUsize>,
}

impl<T: Send + Sync> ObservableRegistry<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ConcurrentRegistry::new()),
            listeners: Vec::new(),
            notify_error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ConcurrentRegistry::with_capacity(capacity)),
            listeners: Vec::new(),
            notify_error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Register a listener that will be notified on every mutation.
    pub fn add_listener(&mut self, listener: Arc<dyn RegistryEventListener>) {
        self.listeners.push(listener);
    }

    /// Number of listener notification errors since last reset.
    pub fn notify_error_count(&self) -> usize {
        self.notify_error_count.load(Ordering::Relaxed)
    }

    fn dispatch_registered(&self, key: &str) {
        let key = key.to_string();
        let listeners = self.listeners.clone();
        tokio::spawn(async move {
            for listener in &listeners {
                listener.on_registered(&key).await;
            }
        });
    }

    fn dispatch_unregistered(&self, key: &str) {
        let key = key.to_string();
        let listeners = self.listeners.clone();
        tokio::spawn(async move {
            for listener in &listeners {
                listener.on_unregistered(&key).await;
            }
        });
    }
}

impl<T: Send + Sync> Default for ObservableRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Registry<T> for ObservableRegistry<T> {
    fn get(&self, key: &str) -> Option<Arc<T>> {
        self.inner.get(key)
    }

    fn has(&self, key: &str) -> bool {
        self.inner.has(key)
    }

    fn list(&self) -> Vec<String> {
        self.inner.list()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<T: Send + Sync> MutableRegistry<T> for ObservableRegistry<T> {
    fn register(&self, key: String, item: Arc<T>) -> RegistryResult<()> {
        let result = self.inner.register(key.clone(), item);
        if result.is_ok() {
            self.dispatch_registered(&key);
        }
        result
    }

    fn register_or_replace(&self, key: String, item: Arc<T>) -> Option<Arc<T>> {
        let result = self.inner.register_or_replace(key.clone(), item);
        self.dispatch_registered(&key);
        result
    }

    fn unregister(&self, key: &str) -> Option<Arc<T>> {
        let result = self.inner.unregister(key);
        if result.is_some() {
            self.dispatch_unregistered(key);
        }
        result
    }

    fn clear(&self) {
        self.inner.clear();
    }
}

impl<T: Send + Sync> BatchRegistry<T> for ObservableRegistry<T> {
    fn register_batch(&self, items: Vec<(String, Arc<T>)>) -> RegistryResult<()> {
        self.inner.register_batch(items)
    }

    fn unregister_batch(&self, keys: &[String]) -> Vec<Option<Arc<T>>> {
        self.inner.unregister_batch(keys)
    }
}
