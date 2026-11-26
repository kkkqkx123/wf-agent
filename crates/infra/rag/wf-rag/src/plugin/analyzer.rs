use super::*;

/// Wraps a plugin analyzer as a tokenizer closure and registers it.
pub fn register_plugin_analyzer(analyzer: Arc<dyn PluginTextAnalyzer>) {
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
    crate::tokenizer::register_analyzer(&trimmed, closure);
}

/// Removes a previously bridged plugin analyzer.
pub fn unregister_plugin_analyzer(name: &str) {
    crate::tokenizer::unregister_analyzer(name);
}
