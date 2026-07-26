use moka::sync::Cache as MokaCache;
use std::time::Duration;

pub struct CheckpointCache<V> {
    inner: MokaCache<String, V>,
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
        }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        self.inner.get(key)
    }

    pub fn put(&self, key: String, value: V) {
        self.inner.insert(key, value);
    }

    pub fn remove(&self, key: &str) {
        self.inner.invalidate(key);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn len(&self) -> u64 {
        self.inner.entry_count()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.entry_count() == 0
    }

    pub fn clear(&self) {
        self.inner.invalidate_all();
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
}
