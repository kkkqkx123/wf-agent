//! Custom per-model pricing override persisted in `wf-storage`.
//!
//! Consulted first by the runtime's composed resolver, ahead of the embedded
//! public LiteLLM table. `provider` is optional: when absent the rule matches
//! any provider for `model`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::entity::Entity;
use crate::domain::store::{FilterOp, QueryFilter};

/// A custom per-model pricing override stored in `wf-storage`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    /// Stable id, e.g. `pricing:{provider}:{model}` or a UUID.
    pub id: String,
    /// Optional provider scoping; `None` matches any provider.
    #[serde(default)]
    pub provider: Option<String>,
    /// Model name (matched exactly against the profile `model`).
    pub model: String,
    /// Price per million input (non-cached) tokens.
    pub input_per_mtok: f64,
    /// Price per million output (completion) tokens.
    pub output_per_mtok: f64,
    /// Price per million cache-read tokens. `None` falls back to input price.
    #[serde(default)]
    pub cache_read_per_mtok: Option<f64>,
    /// Price per million cache-write (5-minute TTL) tokens.
    #[serde(default)]
    pub cache_write_5m_per_mtok: Option<f64>,
    /// Price per million cache-write (1-hour TTL) tokens.
    #[serde(default)]
    pub cache_write_1h_per_mtok: Option<f64>,
    /// How the provider accounts cache tokens relative to `prompt_tokens`.
    pub scope: PricingScope,
}

/// Cache token accounting style for a custom pricing rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PricingScope {
    IncludedInPrompt,
    SeparateFromPrompt,
}

/// List options for `PricingRuleStorage`. Currently only pagination; the
/// composed resolver loads the whole table into a memory mirror so server-side
/// filtering is unnecessary.
#[derive(Debug, Clone, Default)]
pub struct PricingRuleListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

impl From<PricingRuleListOptions> for QueryFilter {
    fn from(opts: PricingRuleListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        filter
    }
}

impl Entity for PricingRule {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "pricing_rule"
    }

    fn metadata(&self) -> Self::Metadata {
        json!({
            "entityType": "pricing_rule",
            "provider": self.provider,
            "model": self.model,
        })
    }
}
