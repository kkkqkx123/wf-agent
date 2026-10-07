//! Query text preprocessing before embedding.
//!
//! Provides generic prefix and template-based preprocessing. Model-specific
//! prefixes are supplied as configuration data by the caller.

use serde::{Deserialize, Serialize};

/// Preprocessor configuration, deserializable from service config files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreprocessorConfig {
    /// No preprocessing (default).
    #[default]
    None,
    /// Prepend a fixed prefix to every text.
    Prefix { prefix: String },
    /// Substitute `{{text}}` into a template.
    Template { template: String },
}

/// Concrete preprocessor built from [`PreprocessorConfig`].
#[derive(Debug, Clone)]
pub enum PreprocessorImpl {
    /// Pass text through unchanged.
    None,
    /// Prepend a fixed prefix.
    Prefix { prefix: String },
    /// Substitute `{{text}}` into a template.
    Template { template: String },
}

impl PreprocessorImpl {
    /// Builds the concrete preprocessor for a config value.
    pub fn from_config(config: &PreprocessorConfig) -> Self {
        match config {
            PreprocessorConfig::None => Self::None,
            PreprocessorConfig::Prefix { prefix } => Self::Prefix {
                prefix: prefix.clone(),
            },
            PreprocessorConfig::Template { template } => Self::Template {
                template: template.clone(),
            },
        }
    }

    /// Applies the preprocessing rule to a single text.
    pub fn preprocess(&self, text: &str) -> String {
        match self {
            Self::None => text.to_string(),
            Self::Prefix { prefix } => format!("{prefix}{text}"),
            Self::Template { template } => template.replace("{{text}}", text),
        }
    }

    /// Applies the preprocessing rule to a batch of texts, preserving order.
    pub fn process_batch(&self, texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| self.preprocess(text)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_passes_text_through() {
        assert_eq!(PreprocessorImpl::None.preprocess("hello"), "hello");
    }

    #[test]
    fn prefix_prepends_literal() {
        let preprocessor = PreprocessorImpl::Prefix {
            prefix: "query: ".into(),
        };
        assert_eq!(preprocessor.preprocess("rust"), "query: rust");
    }

    #[test]
    fn template_substitutes_placeholder() {
        let preprocessor = PreprocessorImpl::Template {
            template: "classify: {{text}}".into(),
        };
        assert_eq!(preprocessor.preprocess("hello"), "classify: hello");
    }

    #[test]
    fn from_config_round_trips_each_variant() {
        let none = PreprocessorImpl::from_config(&PreprocessorConfig::None);
        assert_eq!(none.preprocess("hello"), "hello");

        let prefixed = PreprocessorImpl::from_config(&PreprocessorConfig::Prefix {
            prefix: ">> ".into(),
        });
        assert_eq!(prefixed.preprocess("x"), ">> x");

        let templated = PreprocessorImpl::from_config(&PreprocessorConfig::Template {
            template: "[{{text}}]".into(),
        });
        assert_eq!(templated.preprocess("x"), "[x]");
    }

    #[test]
    fn batch_preserves_order() {
        let preprocessor = PreprocessorImpl::Prefix {
            prefix: "> ".into(),
        };
        assert_eq!(
            preprocessor.process_batch(&["a", "b", "c"]),
            vec!["> a", "> b", "> c"]
        );
    }
}
