//! Multi-scope retrieval: fan out to several collections or namespaces,
//! then fuse once.
//!
//! Each scope runs the standard read path with its own collection and
//! namespace. Fusion keys hits by scope plus chunk id so identical chunk
//! ids in different scopes survive as separate hits.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::{FusionConfig, FusionMode, RagQueryConfig};
use crate::error::{RagError, Result};
use crate::retriever::{
    concat_fusion, reciprocal_rank_fusion, weighted_fusion_with_weights, RetrievedChunk,
};
use wf_plugin_sdk::PluginRouteSelector;

/// One retrieval scope: a collection plus a namespace inside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    /// Target name referenced by routing rules.
    pub name: String,
    /// Vector collection to search.
    pub collection: String,
    /// Namespace scope inside the collection.
    pub namespace: String,
    /// Fusion weight used only by weighted blending.
    #[serde(default = "default_target_weight")]
    pub weight: f32,
}

fn default_target_weight() -> f32 {
    1.0
}

/// Keyword rule selecting targets by query text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    /// Case-insensitive substrings; the first matching rule wins.
    pub query_contains: Vec<String>,
    /// Target names to query when the rule matches.
    pub targets: Vec<String>,
}

/// Multi-scope routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// All known scopes; the fallback when no rule matches.
    pub targets: Vec<RouteTarget>,
    /// Keyword rules checked in order.
    #[serde(default)]
    pub rules: Vec<RouteRule>,
    /// How per-scope hit lists are fused.
    #[serde(default)]
    pub fusion: FusionConfig,
    /// Plugin route selector choosing scopes before keyword rules;
    /// unset keeps rule resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_selector: Option<String>,
}

/// Resolves the targets for one question through the configured plugin
/// route selector, falling back to keyword rules. Unknown selector names
/// fail; failing or empty suggestions fall back to rule resolution.
pub fn resolve_selected<'a>(config: &'a RouterConfig, query: &str) -> Result<Vec<&'a RouteTarget>> {
    let Some(name) = config
        .route_selector
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(resolve_targets(config, query));
    };
    let Some(selector) = crate::plugin::find_route_selector(name) else {
        return Err(RagError::InvalidInput(format!(
            "unknown route selector '{name}'"
        )));
    };
    let candidates: Vec<String> = config
        .targets
        .iter()
        .map(|target| target.name.clone())
        .collect();
    let input = serde_json::json!({
        "protocol_version": crate::plugin::ROUTE_SELECTOR_PROTOCOL_VERSION,
        "query": query,
        "candidates": candidates,
    });
    let output = match selector.select(input) {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(selector = %name, error = %error, "route selector failed; keeping keyword rules");
            return Ok(resolve_targets(config, query));
        }
    };
    let selected: Vec<String> = output
        .get("targets")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let matched: Vec<&RouteTarget> = selected
        .iter()
        .filter_map(|name| config.targets.iter().find(|target| &target.name == name))
        .collect();
    if matched.is_empty() {
        return Ok(resolve_targets(config, query));
    }
    Ok(matched)
}

/// Resolves the targets for one question: the first matching rule wins,
/// otherwise every target. Unknown rule names are skipped; an empty
/// resolution falls back to every target.
pub fn resolve_targets<'a>(config: &'a RouterConfig, query: &str) -> Vec<&'a RouteTarget> {
    let lowered = query.to_lowercase();
    for rule in &config.rules {
        let matched_rule = rule
            .query_contains
            .iter()
            .any(|keyword| !keyword.trim().is_empty() && lowered.contains(&keyword.to_lowercase()));
        if matched_rule {
            let matched: Vec<&RouteTarget> = rule
                .targets
                .iter()
                .filter_map(|name| config.targets.iter().find(|target| &target.name == name))
                .collect();
            if !matched.is_empty() {
                return matched;
            }
        }
    }
    config.targets.iter().collect()
}

/// Fans out one question to the resolved scopes and fuses the hit lists.
pub async fn query_routed<S, E>(
    store: &S,
    embedder: &E,
    router: &RouterConfig,
    query_text: &str,
    query_config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    if router.targets.is_empty() {
        return Err(RagError::InvalidInput("router has no targets".to_string()));
    }
    let mut weighted = Vec::new();
    for target in resolve_selected(router, query_text)? {
        if target.collection.trim().is_empty() || target.namespace.trim().is_empty() {
            return Err(RagError::InvalidInput(format!(
                "route target '{}' has an empty collection or namespace",
                target.name
            )));
        }
        let hits = crate::pipeline::query(
            store,
            embedder,
            &target.collection,
            &target.namespace,
            query_text,
            query_config,
        )
        .await?;
        weighted.push((target.weight.max(0.0), hits));
    }
    let fused = fuse_scoped(weighted, &router.fusion);
    Ok(fused.into_iter().take(query_config.top_k.max(1)).collect())
}

fn fuse_scoped(
    weighted: Vec<(f32, Vec<RetrievedChunk>)>,
    fusion: &FusionConfig,
) -> Vec<RetrievedChunk> {
    let mut scoped: Vec<Vec<RetrievedChunk>> = Vec::with_capacity(weighted.len());
    let mut weights = Vec::with_capacity(weighted.len());
    let mut restore: HashMap<String, String> = HashMap::new();
    for (weight, hits) in weighted {
        weights.push(weight);
        let mut list = Vec::with_capacity(hits.len());
        for mut hit in hits {
            let scoped_id = format!("{}\u{0}{}", hit.namespace, hit.chunk_id);
            restore.insert(scoped_id.clone(), hit.chunk_id.clone());
            hit.chunk_id = scoped_id;
            list.push(hit);
        }
        scoped.push(list);
    }
    let mut fused = match fusion.mode {
        FusionMode::Rrf => reciprocal_rank_fusion(scoped, fusion.rrf_k),
        FusionMode::Concat => concat_fusion(scoped),
        FusionMode::Weighted => weighted_fusion_with_weights(scoped, &weights),
    };
    for hit in &mut fused {
        if let Some(original) = restore.remove(&hit.chunk_id) {
            hit.chunk_id = original;
        }
    }
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, collection: &str, namespace: &str) -> RouteTarget {
        RouteTarget {
            name: name.to_string(),
            collection: collection.to_string(),
            namespace: namespace.to_string(),
            weight: 1.0,
        }
    }

    fn router() -> RouterConfig {
        RouterConfig {
            targets: vec![
                target("docs", "kb-docs", "docs"),
                target("code", "kb-code", "code"),
            ],
            rules: vec![RouteRule {
                query_contains: vec!["fn ".to_string(), "struct ".to_string()],
                targets: vec!["code".to_string()],
            }],
            fusion: FusionConfig::default(),
            route_selector: None,
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let config = router();
        let resolved = resolve_targets(&config, "what does struct Foo do?");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "code");
    }

    #[test]
    fn no_rule_match_queries_every_target() {
        let config = router();
        let resolved = resolve_targets(&config, "how does ownership work?");
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn unknown_rule_names_fall_back_to_everything() {
        let mut config = router();
        config.rules[0].targets = vec!["missing".to_string()];
        assert_eq!(resolve_targets(&config, "struct Foo").len(), 2);
    }

    struct CodeSelector;

    impl wf_plugin_sdk::PluginRouteSelector for CodeSelector {
        fn select(
            &self,
            _input: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Ok(serde_json::json!({"targets": ["code", "missing"]}))
        }
    }

    struct FailingSelector;

    impl wf_plugin_sdk::PluginRouteSelector for FailingSelector {
        fn select(
            &self,
            _input: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
        }
    }

    #[test]
    fn selector_prefers_plugin_subset_and_validates_names() {
        crate::plugin::clear_route_selectors();
        crate::plugin::register_route_selector("code", std::sync::Arc::new(CodeSelector));
        let mut config = router();
        config.route_selector = Some("code".to_string());
        let resolved = resolve_selected(&config, "ownership").expect("selector resolves");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "code");
        config.route_selector = Some("missing-selector".to_string());
        assert!(resolve_selected(&config, "ownership").is_err());
        crate::plugin::register_route_selector("fail", std::sync::Arc::new(FailingSelector));
        config.route_selector = Some("fail".to_string());
        let fallback = resolve_selected(&config, "struct Foo").expect("failure falls back");
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].name, "code");
        crate::plugin::clear_route_selectors();
    }

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait::async_trait]
    impl llm_embedding::provider::EmbeddingProvider for UnitEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings: texts.iter().map(|text| self.vector_for(text)).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "test-unit"
        }
    }

    impl UnitEmbedder {
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut vector = vec![0.0; self.dimension];
            for (index, byte) in text.bytes().enumerate() {
                vector[index % self.dimension] += f32::from(byte) / 255.0;
            }
            vector
        }
    }

    fn doc(id: &str, source: &str, content: &str) -> crate::RagDocument {
        crate::RagDocument::new(id, content, source)
    }

    async fn seeded_store() -> (vector_contracts::InMemoryVectorStore, UnitEmbedder) {
        let store = vector_contracts::InMemoryVectorStore::new();
        let keyword =
            crate::sqlite_index::SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        for (collection, namespace) in [("kb-docs", "docs"), ("kb-code", "code")] {
            crate::pipeline::ingest_documents(
                &store,
                Some(&keyword),
                &embedder,
                collection,
                &[doc(
                    "d1",
                    "guide.md",
                    "ownership borrowing guide text for retrieval",
                )],
                &crate::RagIndexConfig::new(namespace),
            )
            .await
            .expect("ingest");
        }
        (store, embedder)
    }

    #[tokio::test]
    async fn routed_query_covers_every_scope() {
        let (store, embedder) = seeded_store().await;
        let hits = query_routed(
            &store,
            &embedder,
            &router(),
            "ownership borrowing",
            &crate::RagQueryConfig::default(),
        )
        .await
        .expect("query");
        let namespaces: std::collections::HashSet<&str> =
            hits.iter().map(|hit| hit.namespace.as_str()).collect();
        assert!(namespaces.contains("docs"));
        assert!(namespaces.contains("code"));
    }

    #[tokio::test]
    async fn routed_query_rejects_bad_config() {
        let (store, embedder) = seeded_store().await;
        let empty = RouterConfig {
            targets: Vec::new(),
            rules: Vec::new(),
            fusion: FusionConfig::default(),
            route_selector: None,
        };
        assert!(query_routed(
            &store,
            &embedder,
            &empty,
            "ownership",
            &crate::RagQueryConfig::default()
        )
        .await
        .is_err());
        assert!(query_routed(
            &store,
            &embedder,
            &router(),
            "   ",
            &crate::RagQueryConfig::default()
        )
        .await
        .is_err());
    }
}
