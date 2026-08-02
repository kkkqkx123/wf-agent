//! Runtime formatter registry
//!
//! Resolves a formatter for a provider at request time. The five built-in
//! providers are pre-registered; custom providers register their own
//! `Arc<dyn LlmFormatter>` and are addressed through
//! `LlmProvider::Custom(name)` on the profile (plugin extension point).
//!
//! Lookups are case-insensitive. Built-in canonical names (`OPENAI_CHAT`,
//! `OPENAI_RESPONSE`, `ANTHROPIC`, `GEMINI_NATIVE`, `GEMINI_OPENAI`) cannot
//! be overridden by custom registrations.

use std::sync::Arc;

use dashmap::DashMap;
use wf_types::llm::LlmProvider;

use crate::error::{LlmError, LlmResult};
use crate::formatters::{create_formatter, LlmFormatter};

const BUILTIN_PROVIDERS: [&str; 5] = [
    "OPENAI_CHAT",
    "OPENAI_RESPONSE",
    "ANTHROPIC",
    "GEMINI_NATIVE",
    "GEMINI_OPENAI",
];

/// Thread-safe registry of formatters keyed by normalized provider name.
#[derive(Clone)]
pub struct FormatterRegistry {
    formatters: Arc<DashMap<String, Arc<dyn LlmFormatter>>>,
}

impl FormatterRegistry {
    /// Create a registry pre-populated with the five built-in formatters.
    pub fn new() -> Self {
        let formatters: Arc<DashMap<String, Arc<dyn LlmFormatter>>> = Arc::new(DashMap::new());
        for provider in [
            LlmProvider::OpenaiChat,
            LlmProvider::OpenaiResponse,
            LlmProvider::Anthropic,
            LlmProvider::GeminiNative,
            LlmProvider::GeminiOpenai,
        ] {
            if let Ok(formatter) = create_formatter(&provider) {
                formatters.insert(provider.as_str().to_string(), formatter);
            }
        }
        Self { formatters }
    }

    /// Register a custom formatter under a provider name. Built-in canonical
    /// names are rejected.
    pub fn register(&self, name: &str, formatter: Arc<dyn LlmFormatter>) -> LlmResult<()> {
        let normalized = name.to_uppercase();
        if BUILTIN_PROVIDERS.contains(&normalized.as_str()) {
            return Err(LlmError::ConfigError(format!(
                "Cannot register formatter: {} is a built-in provider",
                name
            )));
        }
        if normalized.is_empty() {
            return Err(LlmError::ConfigError(
                "Cannot register formatter with an empty name".to_string(),
            ));
        }
        self.formatters.insert(normalized, formatter);
        Ok(())
    }

    /// Remove a custom formatter. Built-ins cannot be unregistered; the call
    /// is a no-op for them.
    pub fn unregister(&self, name: &str) -> bool {
        let normalized = name.to_uppercase();
        if BUILTIN_PROVIDERS.contains(&normalized.as_str()) {
            return false;
        }
        self.formatters.remove(&normalized).is_some()
    }

    /// Resolve the formatter for a provider: built-ins resolve through the
    /// static factory, `Custom(name)` through the registry.
    pub fn get_by_provider(&self, provider: &LlmProvider) -> LlmResult<Arc<dyn LlmFormatter>> {
        match provider {
            LlmProvider::Custom(name) => self
                .formatters
                .get(&name.to_uppercase())
                .map(|entry| entry.clone())
                .ok_or_else(|| LlmError::FormatterNotFound(name.clone())),
            builtin => create_formatter(builtin),
        }
    }

    /// Whether a formatter (built-in or custom) is available under `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.formatters.contains_key(&name.to_uppercase())
    }

    /// Number of registered formatters (built-ins included).
    pub fn len(&self) -> usize {
        self.formatters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.formatters.is_empty()
    }

    /// Names of all registered formatters (custom + built-in).
    pub fn registered_names(&self) -> Vec<String> {
        self.formatters
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
}

impl Default for FormatterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatters::OpenaiChatFormatter;

    fn make_custom_formatter() -> Arc<dyn LlmFormatter> {
        Arc::new(OpenaiChatFormatter::new())
    }

    #[test]
    fn builtins_pre_registered() {
        let registry = FormatterRegistry::new();
        assert_eq!(registry.len(), 5);
        for name in BUILTIN_PROVIDERS {
            assert!(registry.contains(name));
        }
        assert!(registry.get_by_provider(&LlmProvider::OpenaiChat).is_ok());
        assert!(registry.get_by_provider(&LlmProvider::GeminiOpenai).is_ok());
    }

    #[test]
    fn register_and_resolve_custom() {
        let registry = FormatterRegistry::new();
        registry
            .register("my_provider", make_custom_formatter())
            .expect("custom registration must succeed");
        assert_eq!(registry.len(), 6);

        let resolved = registry
            .get_by_provider(&LlmProvider::Custom("MY_PROVIDER".to_string()))
            .expect("case-insensitive lookup must succeed");
        let resolved2 = registry
            .get_by_provider(&LlmProvider::Custom("my_provider".to_string()))
            .unwrap();
        assert!(Arc::ptr_eq(&resolved, &resolved2));
    }

    #[test]
    fn builtin_names_cannot_be_overridden() {
        let registry = FormatterRegistry::new();
        let err = match registry.register("OPENAI_CHAT", make_custom_formatter()) {
            Err(e) => e,
            Ok(_) => panic!("built-in name must be rejected"),
        };
        assert!(matches!(err, LlmError::ConfigError(_)));
        assert_eq!(registry.len(), 5);
        assert!(!registry.unregister("anthropic"));
    }

    #[test]
    fn empty_name_rejected() {
        let registry = FormatterRegistry::new();
        assert!(registry.register("", make_custom_formatter()).is_err());
    }

    #[test]
    fn unregistered_custom_errors() {
        let registry = FormatterRegistry::new();
        let err = match registry.get_by_provider(&LlmProvider::Custom("nope".to_string())) {
            Err(e) => e,
            Ok(_) => panic!("unregistered provider must error"),
        };
        assert!(matches!(err, LlmError::FormatterNotFound(_)));
    }

    #[test]
    fn unregister_custom() {
        let registry = FormatterRegistry::new();
        registry.register("tmp", make_custom_formatter()).unwrap();
        assert!(registry.unregister("TMP"));
        assert!(!registry.contains("tmp"));
        assert!(!registry.unregister("TMP"));
    }
}
