use std::collections::HashMap;
use std::time::Duration;

use moka::sync::Cache as MokaCache;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ConditionCacheConfig {
    pub max_compilation_entries: u64,
    pub max_execution_entries: u64,
    pub time_to_live: Option<Duration>,
}

impl Default for ConditionCacheConfig {
    fn default() -> Self {
        Self {
            max_compilation_entries: 10_000,
            max_execution_entries: 10_000,
            time_to_live: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CompilationKey(String);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ExecutionKey {
    condition_hash: String,
    context_hash: String,
}

pub struct ConditionCache {
    compilation_cache: MokaCache<CompilationKey, ()>,
    execution_cache: MokaCache<ExecutionKey, bool>,
}

impl ConditionCache {
    pub fn new(config: ConditionCacheConfig) -> Self {
        let compilation_cache = MokaCache::builder()
            .max_capacity(config.max_compilation_entries)
            .time_to_live(config.time_to_live.unwrap_or(Duration::from_secs(3600)))
            .build();

        let execution_cache = MokaCache::builder()
            .max_capacity(config.max_execution_entries)
            .time_to_live(config.time_to_live.unwrap_or(Duration::from_secs(3600)))
            .build();

        Self {
            compilation_cache,
            execution_cache,
        }
    }

    pub fn check_compilation_cache(&self, condition: &str) -> bool {
        let key = CompilationKey(condition.to_string());
        self.compilation_cache.contains_key(&key)
    }

    pub fn record_compilation(&self, condition: &str) {
        let key = CompilationKey(condition.to_string());
        self.compilation_cache.insert(key, ());
    }

    pub fn get_execution_result(
        &self,
        condition: &str,
        context: &HashMap<String, Value>,
    ) -> Option<bool> {
        let context_hash = hash_context(context);
        let key = ExecutionKey {
            condition_hash: condition.to_string(),
            context_hash,
        };
        self.execution_cache.get(&key)
    }

    pub fn put_execution_result(
        &self,
        condition: &str,
        context: &HashMap<String, Value>,
        result: bool,
    ) {
        let context_hash = hash_context(context);
        let key = ExecutionKey {
            condition_hash: condition.to_string(),
            context_hash,
        };
        self.execution_cache.insert(key, result);
    }

    pub fn invalidate(&self, condition: &str) {
        self.compilation_cache
            .invalidate(&CompilationKey(condition.to_string()));
        let to_remove: Vec<_> = self
            .execution_cache
            .iter()
            .filter(|(k, _)| k.condition_hash == condition)
            .map(|(k, _)| (*k).clone())
            .collect();
        for key in to_remove {
            self.execution_cache.invalidate(&key);
        }
    }

    pub fn clear(&self) {
        self.compilation_cache.invalidate_all();
        self.execution_cache.invalidate_all();
    }

    pub fn entry_count(&self) -> usize {
        self.compilation_cache.entry_count() as usize
            + self.execution_cache.entry_count() as usize
    }
}

fn hash_context(context: &HashMap<String, Value>) -> String {
    let serialized = serde_json::to_string(context).unwrap_or_default();
    let hash = blake3::hash(serialized.as_bytes());
    hash.to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compilation_cache() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        assert!(!cache.check_compilation_cache("eq(a, b)"));
        cache.record_compilation("eq(a, b)");
        assert!(cache.check_compilation_cache("eq(a, b)"));
    }

    #[test]
    fn test_execution_cache() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        assert!(cache.get_execution_result("eq(a, b)", &ctx).is_none());
        cache.put_execution_result("eq(a, b)", &ctx, true);
        assert_eq!(cache.get_execution_result("eq(a, b)", &ctx), Some(true));
    }

    #[test]
    fn test_invalidate() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        cache.record_compilation("eq(a, b)");
        cache.put_execution_result("eq(a, b)", &ctx, true);

        cache.invalidate("eq(a, b)");

        assert!(!cache.check_compilation_cache("eq(a, b)"));
        assert!(cache.get_execution_result("eq(a, b)", &ctx).is_none());
    }

    #[test]
    fn test_clear() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        cache.record_compilation("eq(a, b)");
        cache.record_compilation("eq(c, d)");
        cache.put_execution_result("eq(a, b)", &ctx, true);

        cache.clear();
        assert!(!cache.check_compilation_cache("eq(a, b)"));
        assert!(!cache.check_compilation_cache("eq(c, d)"));
    }

    #[test]
    fn test_different_contexts_different_results() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let mut ctx1 = HashMap::new();
        ctx1.insert("x".to_string(), Value::Number(1.into()));
        let mut ctx2 = HashMap::new();
        ctx2.insert("x".to_string(), Value::Number(2.into()));

        cache.put_execution_result("gt(x, 0)", &ctx1, true);
        cache.put_execution_result("gt(x, 0)", &ctx2, true);

        assert_eq!(cache.get_execution_result("gt(x, 0)", &ctx1), Some(true));
        assert_eq!(cache.get_execution_result("gt(x, 0)", &ctx2), Some(true));
    }
}
