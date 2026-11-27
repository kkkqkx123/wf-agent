//! Remote Elasticsearch backend over the REST API.
//!
//! Deliberately thin: only the calls keyword retrieval needs (create,
//! bulk index, match search, delete by query, count) plus a minimal
//! operations set (health, version). No official client dependency; the
//! crate stays on the shared `reqwest` stack, mirroring the Qdrant backend
//! in `vector-contracts`.
//!
//! One index carries every collection; the document id is the deterministic
//! `collection#chunk_id` string so upserts overwrite in one request with no
//! delete-before-insert window. Writes request refresh so a write followed
//! by a read observes the write. Only idempotent reads retry on rate
//! limiting and transient server errors; writes never retry implicitly.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use crate::{
    KeywordDocument, KeywordError, KeywordFormat, KeywordIndex, KeywordQuery, KeywordScope,
    Result as KeywordResult,
};
use serde_json::json;
use vector_contracts::{diagnostic_body, ScoredPoint};

/// Upper bound for automatic read retries on rate limiting and transient faults.
const MAX_READ_RETRIES: usize = 3;

mod schema;
mod store;
#[cfg(test)]
mod test;
mod transport;

use crate::ElasticKeywordConfig;

/// Remote Elasticsearch keyword backend.
pub struct ElasticKeywordIndex {
    client: reqwest::Client,
    config: ElasticKeywordConfig,
}

impl ElasticKeywordIndex {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: &ElasticKeywordConfig) -> KeywordResult<Self> {
        let base_url = config.base_url.trim();
        if base_url.is_empty() {
            return Err(KeywordError::invalid_input(
                "elastic base_url must not be empty".to_string(),
            ));
        }
        if let Some(name) = config
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            crate::tokenizer::ensure_analyzer_registered(name)
                .map_err(KeywordError::InvalidInput)?;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|err| KeywordError::backend(err.to_string()))?;
        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    /// Effective config (used by diagnostics and tests).
    pub fn config(&self) -> &ElasticKeywordConfig {
        &self.config
    }

    /// Deterministic document id for a contracts id pair.
    pub fn doc_id(collection: &str, chunk_id: &str) -> String {
        format!("{collection}#{chunk_id}")
    }

    fn index_name(&self) -> &str {
        self.config.index.trim()
    }
}
