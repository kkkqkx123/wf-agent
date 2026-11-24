//! Process-wide compaction engine factory registry.
//!
//! The registry answers "which engines are available" by name. It holds
//! factories, not live engines: a concrete engine instance is bound once per
//! execution and frozen for that run. Registering a duplicate name fails so
//! a plugin can never silently shadow the built-in engine.

use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use super::builtin::BuiltinEngine;
use super::engine::{
    CompactionEngine, CompactionEngineFactory, BUILTIN_ENGINE_NAME, ENGINE_API_VERSION,
};

static ENGINE_FACTORIES: OnceLock<DashMap<String, Arc<dyn CompactionEngineFactory>>> =
    OnceLock::new();

fn factories() -> &'static DashMap<String, Arc<dyn CompactionEngineFactory>> {
    ENGINE_FACTORIES.get_or_init(DashMap::new)
}

/// Register a native engine factory under its declared name. Returns false
/// when the name is empty, collides with the built-in, or is already taken.
pub fn register_compaction_engine(factory: Arc<dyn CompactionEngineFactory>) -> Result<(), String> {
    let name = factory.name();
    let key = name.trim().to_string();
    if key.is_empty() {
        return Err("compaction engine name is empty".to_string());
    }
    if key == BUILTIN_ENGINE_NAME {
        return Err("compaction engine name 'builtin' is reserved".to_string());
    }
    if factory.api_version() != ENGINE_API_VERSION {
        return Err(format!(
            "compaction engine '{key}' uses api version {}, expected {ENGINE_API_VERSION}",
            factory.api_version()
        ));
    }
    let map = factories();
    if map.contains_key(&key) {
        return Err(format!("compaction engine '{key}' is already registered"));
    }
    map.insert(key, factory);
    Ok(())
}

/// Remove a registered engine factory by name. Returns whether one existed.
pub fn unregister_compaction_engine(name: &str) -> bool {
    factories().remove(name.trim()).is_some()
}

/// Whether a name resolves to an engine (built-in or registered factory).
pub fn has_compaction_engine(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || trimmed == BUILTIN_ENGINE_NAME || factories().contains_key(trimmed)
}

/// Build a fresh engine instance for a name. Unknown names return `None` so
/// the caller can fail loudly at startup instead of silently falling back.
pub fn build_compaction_engine(name: &str) -> Option<Arc<dyn CompactionEngine>> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == BUILTIN_ENGINE_NAME {
        return Some(Arc::new(BuiltinEngine));
    }
    factories()
        .get(trimmed)
        .and_then(|factory| factory.build(serde_json::Value::Null).ok())
}

/// Build a fresh engine instance with private configuration for a name.
/// The built-in engine ignores the configuration object.
pub fn build_compaction_engine_with_config(
    name: &str,
    config: serde_json::Value,
) -> Option<Arc<dyn CompactionEngine>> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == BUILTIN_ENGINE_NAME {
        return Some(Arc::new(BuiltinEngine));
    }
    factories()
        .get(trimmed)
        .and_then(|factory| factory.build(config).ok())
}

/// Snapshot of all registered engine names, including the built-in. Useful
/// for startup validation diagnostics and settings surfaces.
pub fn registered_compaction_engines() -> Vec<String> {
    let mut names = vec![BUILTIN_ENGINE_NAME.to_string()];
    names.extend(factories().iter().map(|entry| entry.key().clone()));
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, OnceLock};

    use async_trait::async_trait;
    use wf_types::message::SeqRange;

    use super::*;
    use crate::compaction::engine::{
        CompactionCall, CompactionEngine, CompactionEngineResult, CompactionOutcome,
        EngineCapabilities, ManualRequest, ProgressSink, ENGINE_API_VERSION,
    };

    // The factory registry is process-wide; serialize tests that mutate it.
    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct DecliningEngine {
        name: String,
    }

    #[async_trait]
    impl CompactionEngine for DecliningEngine {
        fn name(&self) -> &str {
            &self.name
        }
        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities::default()
        }
        async fn compact_if_needed(
            &self,
            _call: &CompactionCall<'_>,
            _progress: &dyn ProgressSink,
        ) -> CompactionEngineResult<Option<CompactionOutcome>> {
            Ok(None)
        }
        async fn compact_now(
            &self,
            _call: &CompactionCall<'_>,
            _progress: &dyn ProgressSink,
            _request: &ManualRequest,
        ) -> CompactionEngineResult<CompactionOutcome> {
            Ok(CompactionOutcome::Noop)
        }
        async fn compact_region(
            &self,
            _call: &CompactionCall<'_>,
            _progress: &dyn ProgressSink,
            _range: SeqRange,
        ) -> CompactionEngineResult<CompactionOutcome> {
            Ok(CompactionOutcome::Noop)
        }
    }

    struct TestFactory {
        name: String,
        version: u32,
    }

    impl CompactionEngineFactory for TestFactory {
        fn name(&self) -> String {
            self.name.clone()
        }
        fn api_version(&self) -> u32 {
            self.version
        }
        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities::default()
        }
        fn build(
            &self,
            _config: serde_json::Value,
        ) -> CompactionEngineResult<Arc<dyn CompactionEngine>> {
            Ok(Arc::new(DecliningEngine {
                name: self.name.clone(),
            }))
        }
    }

    #[test]
    fn builtin_always_resolves() {
        let engine = build_compaction_engine("builtin").expect("builtin resolves");
        assert_eq!(engine.name(), BUILTIN_ENGINE_NAME);
        assert!(build_compaction_engine("").is_some());
    }

    #[test]
    fn factory_registers_builds_and_rejects_duplicates() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let name = "test-factory-register";
        unregister_compaction_engine(name);
        assert!(!has_compaction_engine(name));
        assert!(register_compaction_engine(Arc::new(TestFactory {
            name: name.to_string(),
            version: ENGINE_API_VERSION,
        }))
        .is_ok());
        assert!(has_compaction_engine(name));
        assert!(register_compaction_engine(Arc::new(TestFactory {
            name: name.to_string(),
            version: ENGINE_API_VERSION,
        }))
        .is_err());
        let engine = build_compaction_engine(name).expect("registered engine builds");
        assert_eq!(engine.name(), name);
        assert!(registered_compaction_engines().iter().any(|n| n == name));
        assert!(unregister_compaction_engine(name));
        assert!(!has_compaction_engine(name));
    }

    #[test]
    fn reserved_name_and_version_mismatch_are_rejected() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let reserved = register_compaction_engine(Arc::new(TestFactory {
            name: BUILTIN_ENGINE_NAME.to_string(),
            version: ENGINE_API_VERSION,
        }));
        assert!(reserved.is_err(), "builtin name is reserved");
        let mismatched = register_compaction_engine(Arc::new(TestFactory {
            name: "test-factory-version".to_string(),
            version: ENGINE_API_VERSION + 1,
        }));
        assert!(mismatched.is_err(), "api version mismatch rejected");
    }
}
