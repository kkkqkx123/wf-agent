//! Provider trait and result types for embeddings.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{EmbeddingError, Result};

/// Result of an embedding operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EmbeddingResult {
    /// One embedding vector per input text, in input order.
    pub embeddings: Vec<Vec<f32>>,
    /// Number of prompt tokens reported by the provider.
    pub prompt_tokens: u64,
    /// Total number of tokens reported by the provider.
    pub total_tokens: u64,
}

/// Port for text-to-vector embedding capability.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embeds a batch of texts, returning vectors in input order.
    async fn embed(&self, texts: &[String]) -> Result<EmbeddingResult>;

    /// Embeds a single text.
    async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let result = self.embed(&[text.to_string()]).await?;
        result
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Decode("provider returned no embedding".into()))
    }

    /// Expected vector dimension of this provider.
    fn dimension(&self) -> usize;

    /// Model name served by this provider.
    fn model_name(&self) -> &str;
}
