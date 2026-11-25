//! Converter registry used by source ingestion.
//!
//! The converter port and built-in implementations live in
//! `source-contracts`. This module owns RAG-side selection: explicit names
//! win, and extension matching is only used when a single converter claims
//! the extension.

use std::sync::Arc;

use source_contracts::SourceError;

pub use source_contracts::{
    converter_for, ConvertedDocument, DocumentConverter, JsonConverter, MarkdownConverter,
    PathJsonConverter, PlainConverter,
};

use crate::error::{RagError, Result};

/// Ordered set of converters available to a host.
#[derive(Clone)]
pub struct ConverterRegistry {
    converters: Vec<Arc<dyn DocumentConverter>>,
}

impl Default for ConverterRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl ConverterRegistry {
    /// Builds a registry with the deterministic built-in converters.
    /// Markdown is registered before plain so titles are preserved.
    pub fn with_defaults() -> Self {
        Self {
            converters: vec![
                Arc::new(MarkdownConverter),
                Arc::new(PathJsonConverter),
                Arc::new(JsonConverter),
                Arc::new(PlainConverter),
            ],
        }
    }

    /// Registers a converter ahead of built-ins so explicit extensions can
    /// be claimed by host implementations first.
    pub fn register(&mut self, converter: Arc<dyn DocumentConverter>) -> Result<()> {
        if converter.name().trim().is_empty() {
            return Err(RagError::InvalidInput(
                "converter name must not be empty".to_string(),
            ));
        }
        if self.find(converter.name()).is_some() {
            return Err(RagError::InvalidInput(format!(
                "converter '{}' is already registered",
                converter.name()
            )));
        }
        self.converters.insert(0, converter);
        Ok(())
    }

    /// Finds a converter by name.
    pub fn find(&self, name: &str) -> Option<Arc<dyn DocumentConverter>> {
        self.converters
            .iter()
            .find(|converter| converter.name() == name)
            .cloned()
    }

    /// Resolves a converter from an explicit name or a filename extension.
    pub fn select(
        &self,
        filename: &str,
        explicit: Option<&str>,
    ) -> Result<Arc<dyn DocumentConverter>> {
        if let Some(name) = explicit.filter(|name| !name.trim().is_empty()) {
            return self
                .find(name)
                .ok_or_else(|| RagError::InvalidInput(format!("unknown converter '{name}'")));
        }
        let extension = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        let mut matched = self.converters.iter().filter(|converter| {
            converter
                .extensions()
                .iter()
                .any(|known| *known == extension)
        });
        let first = matched.next().cloned().ok_or_else(|| {
            RagError::InvalidInput(format!("no converter claims extension '{extension}'"))
        })?;
        if matched.next().is_some() {
            return Err(RagError::InvalidInput(format!(
                "multiple converters claim extension '{extension}'; set an explicit converter"
            )));
        }
        Ok(first)
    }

    /// Converts bytes through the named converter.
    pub async fn convert(
        &self,
        filename: &str,
        explicit: Option<&str>,
        bytes: &[u8],
    ) -> Result<ConvertedDocument> {
        let converter = self.select(filename, explicit)?;
        converter
            .convert(filename, bytes)
            .await
            .map_err(|err| match err {
                SourceError::UnknownConverter(name) => {
                    RagError::InvalidInput(format!("unknown converter '{name}'"))
                }
                other => RagError::Conversion(other.to_string()),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_name_wins_and_unknown_fails() {
        let registry = ConverterRegistry::with_defaults();
        assert_eq!(
            registry
                .select("a.json", Some("markdown"))
                .expect("markdown")
                .name(),
            "markdown"
        );
        assert!(registry.select("a.json", Some("missing")).is_err());
    }

    #[tokio::test]
    async fn duplicate_extension_requires_explicit_name() {
        let registry = ConverterRegistry::with_defaults();
        assert!(registry.select("a.json", None).is_err());
        assert_eq!(
            registry
                .select("a.json", Some("json-paths"))
                .expect("json paths")
                .name(),
            "json-paths"
        );
    }
}
