use super::*;

/// Wraps a plugin analyzer as a tokenizer closure and registers it.
pub fn register_plugin_analyzer(
    extensions: &RetrievalExtensions,
    analyzer: Arc<dyn PluginTextAnalyzer>,
) {
    let name = analyzer.name();
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    let label = trimmed.clone();
    let closure: crate::tokenizer::CustomAnalyzer = Arc::new(move |text: String| {
        match analyzer.analyze(text) {
            Ok(terms) => terms
                .into_iter()
                .map(|term| term.trim().to_string())
                .filter(|term| !term.is_empty())
                .collect(),
            Err(error) => {
                tracing::warn!(analyzer = %label, error = %error, "plugin analyzer failed; matching nothing");
                Vec::new()
            }
        }
    });
    extensions.analyzer_registry().register(&trimmed, closure);
}

/// Removes a previously bridged plugin analyzer.
pub fn unregister_plugin_analyzer(extensions: &RetrievalExtensions, name: &str) {
    extensions.analyzer_registry().unregister(name);
}
