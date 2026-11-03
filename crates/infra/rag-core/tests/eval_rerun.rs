//! Rerunnable evaluation set: ingest the bundled documents, run each
//! case through the standard read path and score recall. The set lives in
//! `eval_set.json` so runs stay comparable without an evaluation framework.

use std::collections::HashMap;

use rag_core::{score_case, summarize, EvalCase, RagDocument, RagIndexConfig, RagQueryConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct EvalSet {
    namespace: String,
    collection: String,
    documents: Vec<EvalDocument>,
    cases: Vec<EvalCase>,
}

#[derive(Debug, Deserialize)]
struct EvalDocument {
    id: String,
    source: String,
    content: String,
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
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

#[tokio::test]
async fn eval_set_reruns_with_full_recall() {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/eval_set.json"))
        .expect("eval set loads");
    let set: EvalSet = serde_json::from_str(&raw).expect("eval set parses");
    assert!(!set.cases.is_empty());

    let store = vector_contracts::InMemoryVectorStore::new();
    let keyword = rag_core::MemoryKeywordIndex::new();
    let embedder = UnitEmbedder { dimension: 8 };
    let documents: Vec<RagDocument> = set
        .documents
        .iter()
        .map(|item| RagDocument {
            id: item.id.clone(),
            source: item.source.clone(),
            content: item.content.clone(),
            metadata: HashMap::new(),
        })
        .collect();
    rag_core::pipeline::ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        &set.collection,
        &documents,
        &RagIndexConfig::new(&set.namespace),
    )
    .await
    .expect("ingest");

    let mut outcomes = Vec::new();
    for case in &set.cases {
        let hits = rag_core::pipeline::query(
            &store,
            &embedder,
            &set.collection,
            &case.namespace,
            &case.query,
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        outcomes.push(score_case(case, &hits));
    }
    let summary = summarize(&outcomes);
    assert_eq!(summary.cases, set.cases.len());
    assert_eq!(summary.hit_rate, 1.0, "{outcomes:?}");
    assert_eq!(summary.mean_recall, 1.0, "{outcomes:?}");
}
