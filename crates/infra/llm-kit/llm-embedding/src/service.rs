//! High-level embedding service wrapping a provider.

use crate::provider::{EmbeddingProvider, EmbeddingResult};

/// Convenience wrapper that adds count-based batch handling on top of a provider.
#[derive(Debug, Clone)]
pub struct EmbeddingService<P: EmbeddingProvider> {
    provider: P,
    batch_size: usize,
}

impl<P: EmbeddingProvider> EmbeddingService<P> {
    /// Creates a service with the given provider and batch size.
    pub fn new(provider: P, batch_size: usize) -> Self {
        Self {
            provider,
            batch_size: batch_size.max(1),
        }
    }

    /// Returns the wrapped provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Embeds texts in batches, concatenating results in input order.
    pub async fn embed_batch(&self, texts: &[String]) -> crate::error::Result<EmbeddingResult> {
        let mut all = Vec::new();
        let mut prompt_tokens = 0;
        let mut total_tokens = 0;

        for chunk in texts.chunks(self.batch_size) {
            let result = self.provider.embed(chunk).await?;
            all.extend(result.embeddings);
            prompt_tokens += result.prompt_tokens;
            total_tokens += result.total_tokens;
        }

        Ok(EmbeddingResult {
            embeddings: all,
            prompt_tokens,
            total_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EmbeddingError;
    use async_trait::async_trait;

    struct MockProvider;

    #[async_trait]
    impl EmbeddingProvider for MockProvider {
        async fn embed(&self, texts: &[String]) -> crate::error::Result<EmbeddingResult> {
            if texts.iter().any(|text| text == "boom") {
                return Err(EmbeddingError::Transport("mock failure".into()));
            }
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|text| vec![text.len() as f32]).collect(),
                prompt_tokens: texts.len() as u64,
                total_tokens: texts.len() as u64,
            })
        }

        fn dimension(&self) -> usize {
            1
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn batches_preserve_order_and_sum_usage() {
        let service = EmbeddingService::new(MockProvider, 2);
        let texts: Vec<String> = ["a", "bb", "ccc", "dddd"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let result = service.embed_batch(&texts).await.expect("batch embed");
        assert_eq!(
            result.embeddings,
            vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0]]
        );
        assert_eq!(result.prompt_tokens, 4);
        assert_eq!(result.total_tokens, 4);
    }

    #[tokio::test]
    async fn empty_input_returns_empty_result() {
        let service = EmbeddingService::new(MockProvider, 2);
        let result = service.embed_batch(&[]).await.expect("empty batch");
        assert!(result.embeddings.is_empty());
    }

    #[tokio::test]
    async fn provider_error_propagates() {
        let service = EmbeddingService::new(MockProvider, 2);
        let result = service.embed_batch(&["boom".to_string()]).await;
        assert!(result.is_err());
    }
}
