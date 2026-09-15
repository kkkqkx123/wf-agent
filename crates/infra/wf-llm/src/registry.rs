//! Runtime codec registry
//!
//! Resolves a codec for a wire protocol format at request time. The four
//! built-in formats resolve through the static factory; custom formats
//! register their own `Arc<dyn LlmCodec>` here and are addressed through
//! `LlmFormat::Custom(name)` on the profile (plugin extension point).
//!
//! Only custom codecs live in the map. Built-in canonical names
//! (`OPENAI_CHAT`, `OPENAI_RESPONSE`, `ANTHROPIC`, `GEMINI_NATIVE`) cannot
//! be registered or unregistered.
//!
//! Lookups are case-insensitive.

use std::sync::Arc;

use dashmap::DashMap;
use wf_types::llm::LlmFormat;

use crate::codecs::{create_codec, LlmCodec};
use crate::error::{LlmError, LlmResult};

const BUILTIN_FORMATS: [&str; 4] = [
    "OPENAI_CHAT",
    "OPENAI_RESPONSE",
    "ANTHROPIC",
    "GEMINI_NATIVE",
];

fn is_builtin(name: &str) -> bool {
    BUILTIN_FORMATS.contains(&name.to_uppercase().as_str())
}

/// Thread-safe registry of custom codecs keyed by normalized format name.
#[derive(Clone)]
pub struct CodecRegistry {
    codecs: Arc<DashMap<String, Arc<dyn LlmCodec>>>,
}

impl CodecRegistry {
    /// Create an empty registry. Built-ins need no storage: they resolve
    /// through the static factory.
    pub fn new() -> Self {
        Self {
            codecs: Arc::new(DashMap::new()),
        }
    }

    /// Register a custom codec under a format name. Built-in canonical
    /// names are rejected.
    pub fn register(&self, name: &str, codec: Arc<dyn LlmCodec>) -> LlmResult<()> {
        let normalized = name.to_uppercase();
        if normalized.is_empty() {
            return Err(LlmError::ConfigError(
                "Cannot register codec with an empty name".to_string(),
            ));
        }
        if is_builtin(&normalized) {
            return Err(LlmError::ConfigError(format!(
                "Cannot register codec: {} is a built-in format",
                name
            )));
        }
        self.codecs.insert(normalized, codec);
        Ok(())
    }

    /// Remove a custom codec. Built-ins are never stored, so unregistering
    /// one simply reports `false`.
    pub fn unregister(&self, name: &str) -> bool {
        self.codecs.remove(&name.to_uppercase()).is_some()
    }

    /// Resolve the codec for a format: built-ins resolve through the
    /// static factory, `Custom(name)` through the registry.
    pub fn get_by_format(&self, format: &LlmFormat) -> LlmResult<Arc<dyn LlmCodec>> {
        match format {
            LlmFormat::Custom(name) => self
                .codecs
                .get(&name.to_uppercase())
                .map(|entry| entry.clone())
                .ok_or_else(|| LlmError::CodecNotFound(name.clone())),
            builtin => create_codec(builtin),
        }
    }

    /// Whether a codec (built-in or custom) is available under `name`.
    pub fn contains(&self, name: &str) -> bool {
        is_builtin(name) || self.codecs.contains_key(&name.to_uppercase())
    }

    /// Number of registered codecs (built-ins included).
    pub fn len(&self) -> usize {
        self.codecs.len() + BUILTIN_FORMATS.len()
    }

    /// Always `false`: the four built-ins mean the registry is never empty.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Names of all registered codecs (custom + built-in).
    pub fn registered_names(&self) -> Vec<String> {
        let mut names: Vec<String> = BUILTIN_FORMATS.iter().map(|s| s.to_string()).collect();
        names.extend(self.codecs.iter().map(|entry| entry.key().clone()));
        names
    }
}

impl Default for CodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::OpenaiChatCodec;

    fn make_custom_codec() -> Arc<dyn LlmCodec> {
        Arc::new(OpenaiChatCodec::new())
    }

    #[test]
    fn builtins_resolve_without_registration() {
        let registry = CodecRegistry::new();
        assert_eq!(registry.len(), 4);
        for name in BUILTIN_FORMATS {
            assert!(registry.contains(name));
        }
        assert!(registry.get_by_format(&LlmFormat::OpenaiChat).is_ok());
        assert!(registry.get_by_format(&LlmFormat::GeminiNative).is_ok());
        assert_eq!(registry.registered_names().len(), 4);
    }

    #[test]
    fn register_and_resolve_custom() {
        let registry = CodecRegistry::new();
        registry
            .register("my_provider", make_custom_codec())
            .expect("custom registration must succeed");
        assert_eq!(registry.len(), 5);

        let resolved = registry
            .get_by_format(&LlmFormat::Custom("MY_PROVIDER".to_string()))
            .expect("case-insensitive lookup must succeed");
        let resolved2 = registry
            .get_by_format(&LlmFormat::Custom("my_provider".to_string()))
            .unwrap();
        assert!(Arc::ptr_eq(&resolved, &resolved2));
    }

    #[test]
    fn builtin_names_cannot_be_overridden() {
        let registry = CodecRegistry::new();
        let err = match registry.register("OPENAI_CHAT", make_custom_codec()) {
            Err(e) => e,
            Ok(_) => panic!("built-in name must be rejected"),
        };
        assert!(matches!(err, LlmError::ConfigError(_)));
        assert_eq!(registry.len(), 4);
        assert!(!registry.unregister("anthropic"));
    }

    #[test]
    fn empty_name_rejected() {
        let registry = CodecRegistry::new();
        assert!(registry.register("", make_custom_codec()).is_err());
    }

    #[test]
    fn unregistered_custom_errors() {
        let registry = CodecRegistry::new();
        let err = match registry.get_by_format(&LlmFormat::Custom("nope".to_string())) {
            Err(e) => e,
            Ok(_) => panic!("unregistered format must error"),
        };
        assert!(matches!(err, LlmError::CodecNotFound(_)));
    }

    #[test]
    fn unregister_custom() {
        let registry = CodecRegistry::new();
        registry.register("tmp", make_custom_codec()).unwrap();
        assert!(registry.unregister("TMP"));
        assert!(!registry.contains("tmp"));
        assert!(!registry.unregister("TMP"));
    }
}
