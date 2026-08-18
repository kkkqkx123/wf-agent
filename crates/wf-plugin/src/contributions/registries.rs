use dashmap::DashMap;
use std::borrow::Borrow;
use std::hash::Hash;

struct RegistryEntry<V> {
    plugin_id: String,
    value: V,
}

pub struct Registry<K, V> {
    entries: DashMap<K, RegistryEntry<V>>,
}

impl<K: Hash + Eq, V: Clone> Registry<K, V> {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    pub fn register(&self, key: K, plugin_id: String, value: V) {
        self.entries.insert(key, RegistryEntry { plugin_id, value });
    }

    pub fn has<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.contains_key(key)
    }

    pub fn get_owner<Q>(&self, key: &Q) -> Option<String>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.get(key).map(|e| e.plugin_id.clone())
    }

    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.get(key).map(|e| e.value.clone())
    }

    pub fn unregister_by_plugin(&self, plugin_id: &str) {
        self.entries.retain(|_, e| e.plugin_id != plugin_id);
    }

    pub fn all(&self) -> Vec<(K, String)>
    where
        K: Clone,
    {
        self.entries
            .iter()
            .map(|e| (e.key().clone(), e.value().plugin_id.clone()))
            .collect()
    }
}

impl<K: Hash + Eq, V: Clone> Default for Registry<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Generic multi-value registry (1:N)
// ============================================================

struct MultiRegistryEntry<V> {
    plugin_id: String,
    value: V,
}

pub struct MultiRegistry<K, V> {
    entries: DashMap<K, Vec<MultiRegistryEntry<V>>>,
}

impl<K: Hash + Eq, V: Clone> MultiRegistry<K, V> {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    pub fn register(&self, key: K, plugin_id: String, value: V) {
        self.entries
            .entry(key)
            .or_default()
            .push(MultiRegistryEntry { plugin_id, value });
    }

    pub fn get<Q>(&self, key: &Q) -> Vec<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries
            .get(key)
            .map(|e| e.iter().map(|o| o.value.clone()).collect())
            .unwrap_or_default()
    }

    pub fn unregister_by_plugin(&self, plugin_id: &str) {
        for mut entry in self.entries.iter_mut() {
            entry.retain(|e| e.plugin_id != plugin_id);
        }
        self.entries.retain(|_, v| !v.is_empty());
    }

    pub fn all(&self) -> Vec<(K, String)>
    where
        K: Clone,
    {
        let mut result = Vec::new();
        for entry in self.entries.iter() {
            let key = entry.key().clone();
            for handler in entry.value().iter() {
                result.push((key.clone(), handler.plugin_id.clone()));
            }
        }
        result
    }

    pub fn keys(&self) -> Vec<K>
    where
        K: Clone,
    {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }
}

impl<K: Hash + Eq, V: Clone> Default for MultiRegistry<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
