use moka::sync::Cache as MokaCache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Cache statistics, aligned with the TS `CheckpointCache.getStats()`
/// (`entries`, `hits`, `misses`, `hitRate`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CacheStats {
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

pub struct CheckpointCache<V> {
    inner: MokaCache<String, V>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl<V> CheckpointCache<V>
where
    V: Clone + Send + Sync + 'static,
{
    pub fn new(max_capacity: u64, ttl_seconds: u64) -> Self {
        Self {
            inner: MokaCache::builder()
                .max_capacity(max_capacity)
                .time_to_live(Duration::from_secs(ttl_seconds))
                .build(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let value = self.inner.get(key);
        if value.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        value
    }

    pub fn put(&self, key: String, value: V) {
        self.inner.insert(key, value);
    }

    /// Get the cached value or populate it with the factory on a miss.
    pub async fn get_or_set<F, Fut, E>(&self, key: String, factory: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>>,
    {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let value = factory().await?;
        self.put(key, value.clone());
        Ok(value)
    }

    pub fn remove(&self, key: &str) {
        self.inner.invalidate(key);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn len(&self) -> u64 {
        self.inner.run_pending_tasks();
        self.inner.entry_count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.inner.invalidate_all();
        self.inner.run_pending_tasks();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    pub fn keys(&self) -> Vec<String> {
        self.inner.run_pending_tasks();
        self.inner
            .iter()
            .map(|(k, _)| k.to_string())
            .collect()
    }

    /// Remove all expired entries. Moka expires entries lazily: pending
    /// maintenance is processed and remaining entries are touched to force
    /// expiry. Returns a best-effort count of removed entries (0 when the
    /// maintenance pass already purged them).
    pub fn cleanup(&self) -> u64 {
        self.inner.run_pending_tasks();
        let mut removed = 0;
        for key in self.keys() {
            if self.inner.get(&key).is_none() {
                removed += 1;
            }
        }
        removed
    }

    pub fn get_stats(&self) -> CacheStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        CacheStats {
            entries: self.len(),
            hits,
            misses,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}

impl<V> Default for CheckpointCache<V>
where
    V: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new(1000, 300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_get() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("key1".to_string(), 42u64);
        assert_eq!(cache.get("key1"), Some(42));
    }

    #[test]
    fn get_missing() {
        let cache: CheckpointCache<String> = CheckpointCache::new(100, 60);
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn contains() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("k".to_string(), 1);
        assert!(cache.contains("k"));
        assert!(!cache.contains("other"));
    }

    #[test]
    fn remove() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("k".to_string(), 1);
        cache.remove("k");
        assert!(!cache.contains("k"));
    }

    #[test]
    fn clear() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn stats_track_hits_and_misses() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("a".to_string(), 1);

        assert_eq!(cache.get("a"), Some(1));
        assert_eq!(cache.get("missing"), None);
        assert_eq!(cache.get("a"), Some(1));

        let stats = cache.get_stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 2.0 / 3.0).abs() < 1e-9);

        cache.clear();
        let reset = cache.get_stats();
        assert_eq!(reset.hits, 0);
        assert_eq!(reset.misses, 0);
    }

    #[tokio::test]
    async fn get_or_set_populates_on_miss() {
        let cache = CheckpointCache::new(100, 60);

        let first: u64 = cache
            .get_or_set("k".to_string(), || async { Ok::<_, ()>(42) })
            .await
            .unwrap();
        assert_eq!(first, 42);

        // Second call hits the cache, factory not invoked again.
        let second: u64 = cache
            .get_or_set("k".to_string(), || async {
                panic!("factory must not run on cache hit");
                #[allow(unreachable_code)]
                Ok::<_, ()>(0)
            })
            .await
            .unwrap();
        assert_eq!(second, 42);
    }

    #[test]
    fn keys_lists_all_entries() {
        let cache = CheckpointCache::new(100, 60);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        let mut keys = cache.keys();
        keys.sort();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn cleanup_purges_expired_entries() {
        let cache = CheckpointCache::new(100, 1);
        cache.put("a".to_string(), 1);
        assert_eq!(cache.len(), 1);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        cache.cleanup();
        assert!(
            cache.get("a").is_none(),
            "expired entry is gone after cleanup"
        );
        assert!(cache.is_empty());
    }
}
