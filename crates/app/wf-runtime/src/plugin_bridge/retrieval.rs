use wf_plugin::{ContributionManager, PluginError, PluginResult};

pub(crate) fn sync_retrieval(plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
    for (name, owner) in manager.all_text_analyzers() {
        if owner != plugin_id {
            continue;
        }
        let Some(analyzer) = manager.get_text_analyzer(&name) else {
            continue;
        };
        if rag_core::tokenizer::find_analyzer(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' text analyzer '{name}' conflicts with an existing analyzer"
            )));
        }
        rag_core::plugin::register_plugin_analyzer(analyzer);
        tracing::debug!("  text-analyzer (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_retrieval_advisors() {
        if owner != plugin_id {
            continue;
        }
        let Some(advisor) = manager.get_retrieval_advisor(&name) else {
            continue;
        };
        if rag_core::plugin::find_retrieval_advisor(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' retrieval advisor '{name}' conflicts with an existing advisor"
            )));
        }
        rag_core::plugin::register_retrieval_advisor(&name, advisor);
        tracing::debug!("  retrieval-advisor (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_vector_encodings() {
        if owner != plugin_id {
            continue;
        }
        let Some(encoding) = manager.get_vector_encoding(&name) else {
            continue;
        };
        if rag_core::plugin::find_vector_encoding(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' vector encoding '{name}' conflicts with an existing encoding"
            )));
        }
        rag_core::plugin::register_vector_encoding(&name, encoding);
        tracing::debug!("  vector-encoding (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_snapshot_policies() {
        if owner != plugin_id {
            continue;
        }
        let Some(policy) = manager.get_snapshot_policy(&name) else {
            continue;
        };
        if !rag_core::plugin::is_valid_snapshot_policy(&policy) {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' snapshot policy '{name}' must be a JSON object"
            )));
        }
        rag_core::plugin::register_snapshot_policy(&name, policy);
        tracing::debug!("  snapshot-policy (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_metadata_extractors() {
        if owner != plugin_id {
            continue;
        }
        let Some(extractor) = manager.get_metadata_extractor(&name) else {
            continue;
        };
        if rag_core::plugin::find_metadata_extractor(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' metadata extractor '{name}' conflicts with an existing extractor"
            )));
        }
        rag_core::plugin::register_metadata_extractor(&name, extractor);
        tracing::debug!("  metadata-extractor (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_chunk_splitters() {
        if owner != plugin_id {
            continue;
        }
        let Some(splitter) = manager.get_chunk_splitter(&name) else {
            continue;
        };
        if rag_core::plugin::find_chunk_splitter(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' chunk splitter '{name}' conflicts with an existing splitter"
            )));
        }
        rag_core::plugin::register_chunk_splitter(&name, splitter);
        tracing::debug!("  chunk-splitter (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_route_selectors() {
        if owner != plugin_id {
            continue;
        }
        let Some(selector) = manager.get_route_selector(&name) else {
            continue;
        };
        if rag_core::plugin::find_route_selector(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' route selector '{name}' conflicts with an existing selector"
            )));
        }
        rag_core::plugin::register_route_selector(&name, selector);
        tracing::debug!("  route-selector (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_postprocessors() {
        if owner != plugin_id {
            continue;
        }
        let Some(postprocessor) = manager.get_postprocessor(&name) else {
            continue;
        };
        if rag_core::plugin::find_postprocessor(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' postprocessor '{name}' conflicts with an existing postprocessor"
            )));
        }
        rag_core::plugin::register_postprocessor(&name, postprocessor);
        tracing::debug!("  postprocessor (rag-registered): {}", name);
    }
    for (name, owner) in manager.all_eval_judges() {
        if owner != plugin_id {
            continue;
        }
        let Some(judge) = manager.get_eval_judge(&name) else {
            continue;
        };
        if rag_core::plugin::find_eval_judge(&name).is_some() {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' eval judge '{name}' conflicts with an existing judge"
            )));
        }
        rag_core::plugin::register_eval_judge(&name, judge);
        tracing::debug!("  eval-judge (rag-registered): {}", name);
    }
    Ok(())
}

pub(crate) fn unsync_retrieval(plugin_id: &str, manager: &ContributionManager) {
    for (name, owner) in manager.all_text_analyzers() {
        if owner == plugin_id {
            rag_core::plugin::unregister_plugin_analyzer(&name);
            tracing::debug!("  text-analyzer (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_retrieval_advisors() {
        if owner == plugin_id {
            rag_core::plugin::unregister_retrieval_advisor(&name);
            tracing::debug!("  retrieval-advisor (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_vector_encodings() {
        if owner == plugin_id {
            rag_core::plugin::unregister_vector_encoding(&name);
            tracing::debug!("  vector-encoding (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_snapshot_policies() {
        if owner == plugin_id {
            rag_core::plugin::unregister_snapshot_policy(&name);
            tracing::debug!("  snapshot-policy (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_metadata_extractors() {
        if owner == plugin_id {
            rag_core::plugin::unregister_metadata_extractor(&name);
            tracing::debug!("  metadata-extractor (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_chunk_splitters() {
        if owner == plugin_id {
            rag_core::plugin::unregister_chunk_splitter(&name);
            tracing::debug!("  chunk-splitter (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_route_selectors() {
        if owner == plugin_id {
            rag_core::plugin::unregister_route_selector(&name);
            tracing::debug!("  route-selector (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_postprocessors() {
        if owner == plugin_id {
            rag_core::plugin::unregister_postprocessor(&name);
            tracing::debug!("  postprocessor (rag-removed): {}", name);
        }
    }
    for (name, owner) in manager.all_eval_judges() {
        if owner == plugin_id {
            rag_core::plugin::unregister_eval_judge(&name);
            tracing::debug!("  eval-judge (rag-removed): {}", name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::Arc;
    use wf_plugin::ContributionBridge;
    use wf_plugin::ContributionRegistrar;
    use wf_resource::registry::ResourceRegistries;
    use wf_tools::registry::ToolRegistry;

    use crate::plugin_bridge::bridge::WfPluginBridge;

    struct BridgeAnalyzer;

    impl wf_plugin::PluginTextAnalyzer for BridgeAnalyzer {
        fn name(&self) -> String {
            "bridge-test-analyzer".to_string()
        }

        fn analyze(&self, text: String) -> wf_plugin::PluginResult<Vec<String>> {
            Ok(vec![text])
        }
    }

    struct BridgeAdvisor;

    impl wf_plugin::PluginRetrievalAdvisor for BridgeAdvisor {
        fn advise(&self, input: Value) -> wf_plugin::PluginResult<Value> {
            Ok(input)
        }
    }

    struct BridgeEncoding;

    impl wf_plugin::PluginVectorEncoding for BridgeEncoding {
        fn name(&self) -> String {
            "bridge-test-encoding".to_string()
        }

        fn encode(&self, _text: String, _options: Value) -> wf_plugin::PluginResult<Value> {
            Ok(serde_json::json!({}))
        }
    }

    struct BridgeExtractor;

    impl wf_plugin::PluginMetadataExtractor for BridgeExtractor {
        fn name(&self) -> String {
            "bridge-test-extractor".to_string()
        }

        fn extract(&self, _text: String, _options: Value) -> wf_plugin::PluginResult<Value> {
            Ok(serde_json::json!({}))
        }
    }

    struct BridgeSplitter;

    impl wf_plugin::PluginChunkSplitter for BridgeSplitter {
        fn name(&self) -> String {
            "bridge-test-splitter".to_string()
        }

        fn split(&self, _text: String, _options: Value) -> wf_plugin::PluginResult<Value> {
            Ok(serde_json::json!([]))
        }
    }

    struct BridgeSelector;

    impl wf_plugin::PluginRouteSelector for BridgeSelector {
        fn select(&self, input: Value) -> wf_plugin::PluginResult<Value> {
            Ok(input)
        }
    }

    struct BridgePostprocessor;

    impl wf_plugin::PluginPostprocessor for BridgePostprocessor {
        fn process(&self, input: Value) -> wf_plugin::PluginResult<Value> {
            Ok(input)
        }
    }

    struct BridgeJudge;

    impl wf_plugin::PluginEvalJudge for BridgeJudge {
        fn judge(&self, input: Value) -> wf_plugin::PluginResult<Value> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn sync_retrieval_registers_and_unsync_removes() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        let bridge = WfPluginBridge::new(
            registries,
            tool_registry,
            gateway,
            Arc::new(wf_execution_shared::hooks::HookHandlerRegistry::new()),
            Arc::new(wf_plugin::PluginRegistry::new()),
        );
        let manager = ContributionManager::new();
        manager.start_registration("p9");
        manager
            .as_registrar()
            .register_text_analyzer("bridge-test-analyzer", Arc::new(BridgeAnalyzer))
            .unwrap();
        manager
            .as_registrar()
            .register_retrieval_advisor("bridge-test-advisor", Arc::new(BridgeAdvisor))
            .unwrap();
        manager
            .as_registrar()
            .register_vector_encoding("bridge-test-encoding", Arc::new(BridgeEncoding))
            .unwrap();
        manager
            .as_registrar()
            .register_snapshot_policy("bridge-test-snap", serde_json::json!({"expect": 1}))
            .unwrap();
        manager
            .as_registrar()
            .register_metadata_extractor("bridge-test-extractor", Arc::new(BridgeExtractor))
            .unwrap();
        manager
            .as_registrar()
            .register_chunk_splitter("bridge-test-splitter", Arc::new(BridgeSplitter))
            .unwrap();
        manager
            .as_registrar()
            .register_route_selector("bridge-test-selector", Arc::new(BridgeSelector))
            .unwrap();
        manager
            .as_registrar()
            .register_postprocessor("bridge-test-post", Arc::new(BridgePostprocessor))
            .unwrap();
        manager
            .as_registrar()
            .register_eval_judge("bridge-test-judge", Arc::new(BridgeJudge))
            .unwrap();

        bridge.sync_all("p9", &manager).await.unwrap();
        assert!(rag_core::tokenizer::find_analyzer("bridge-test-analyzer").is_some());
        assert!(rag_core::plugin::find_retrieval_advisor("bridge-test-advisor").is_some());
        assert!(rag_core::plugin::find_vector_encoding("bridge-test-encoding").is_some());
        assert!(rag_core::plugin::find_snapshot_policy("bridge-test-snap").is_some());
        assert!(rag_core::plugin::find_metadata_extractor("bridge-test-extractor").is_some());
        assert!(rag_core::plugin::find_chunk_splitter("bridge-test-splitter").is_some());
        assert!(rag_core::plugin::find_route_selector("bridge-test-selector").is_some());
        assert!(rag_core::plugin::find_postprocessor("bridge-test-post").is_some());
        assert!(rag_core::plugin::find_eval_judge("bridge-test-judge").is_some());

        bridge.unsync_all("p9", &manager).await.unwrap();
        assert!(rag_core::tokenizer::find_analyzer("bridge-test-analyzer").is_none());
        assert!(rag_core::plugin::find_retrieval_advisor("bridge-test-advisor").is_none());
        assert!(rag_core::plugin::find_vector_encoding("bridge-test-encoding").is_none());
        assert!(rag_core::plugin::find_snapshot_policy("bridge-test-snap").is_none());
        assert!(rag_core::plugin::find_metadata_extractor("bridge-test-extractor").is_none());
        assert!(rag_core::plugin::find_chunk_splitter("bridge-test-splitter").is_none());
        assert!(rag_core::plugin::find_route_selector("bridge-test-selector").is_none());
        assert!(rag_core::plugin::find_postprocessor("bridge-test-post").is_none());
        assert!(rag_core::plugin::find_eval_judge("bridge-test-judge").is_none());
    }
}
