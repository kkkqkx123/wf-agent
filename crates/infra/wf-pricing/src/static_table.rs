//! Resolver backed by the embedded LiteLLM public pricing table.

use std::collections::HashMap;

use serde::Deserialize;

use crate::resolver::{ModelKey, PricingResolver};
use llm_token::{CacheTokenScope, ResolvedPricing, TokenPricing};

/// One entry of the embedded LiteLLM public model pricing table.
///
/// Only the fields this resolver needs are mapped; unknown fields are ignored.
/// All costs are stored per-token (LiteLLM's `input_cost_per_token` convention)
/// and converted to per-million-token on read.
#[derive(Debug, Clone, Deserialize)]
struct LiteLlmModel {
    #[serde(default)]
    input_cost_per_token: f64,
    #[serde(default)]
    output_cost_per_token: f64,
    #[serde(default)]
    cache_read_input_token_cost: Option<f64>,
    /// Anthropic-style cache-write (prompt caching creation) cost. LiteLLM
    /// exposes the 5-minute TTL separately; we map it as the 5m write rate.
    #[serde(default)]
    cache_creation_input_token_cost: Option<f64>,
    #[serde(default)]
    cache_creation_input_token_cost_5m: Option<f64>,
    #[serde(default)]
    tiered_pricing: Option<Vec<TieredPricingEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TieredPricingEntry {
    #[serde(default)]
    input_cost_per_token: f64,
    #[serde(default)]
    output_cost_per_token: f64,
    #[serde(default)]
    range: Vec<f64>,
}

impl LiteLlmModel {
    fn to_resolved(&self) -> ResolvedPricing {
        let tiers = self.tiered_pricing.as_ref().map(|entries| {
            entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let up_to_tokens = if index + 1 == entries.len() {
                        None
                    } else {
                        entry
                            .range
                            .get(1)
                            .copied()
                            .filter(|v| *v > 0.0)
                            .map(|v| v as u64)
                    };
                    llm_token::TokenPricingTier {
                        up_to_tokens,
                        input_per_mtok: entry.input_cost_per_token * 1_000_000.0,
                        output_per_mtok: entry.output_cost_per_token * 1_000_000.0,
                    }
                })
                .collect::<Vec<_>>()
        });
        let (base_input, base_output) =
            if self.input_cost_per_token > 0.0 || self.output_cost_per_token > 0.0 {
                (
                    self.input_cost_per_token * 1_000_000.0,
                    self.output_cost_per_token * 1_000_000.0,
                )
            } else if let Some(first) = self.tiered_pricing.as_ref().and_then(|t| t.first()) {
                (
                    first.input_cost_per_token * 1_000_000.0,
                    first.output_cost_per_token * 1_000_000.0,
                )
            } else {
                (0.0, 0.0)
            };
        let input_per_mtok = base_input;
        let output_per_mtok = base_output;
        let cache_read_per_mtok = self.cache_read_input_token_cost.map(|v| v * 1_000_000.0);
        // Prefer the explicit 5m write rate, then the generic creation rate.
        let cache_write_5m_per_mtok = self
            .cache_creation_input_token_cost_5m
            .or(self.cache_creation_input_token_cost)
            .map(|v| v * 1_000_000.0);
        // Presence of a cache-creation cost implies Anthropic-style accounting
        // where prompt_tokens excludes cache tokens.
        let scope = if self.cache_creation_input_token_cost.is_some()
            || self.cache_creation_input_token_cost_5m.is_some()
        {
            CacheTokenScope::SeparateFromPrompt
        } else {
            CacheTokenScope::IncludedInPrompt
        };
        ResolvedPricing {
            pricing: TokenPricing {
                input_per_mtok,
                output_per_mtok,
                cache_read_per_mtok,
                cache_write_5m_per_mtok,
                cache_write_1h_per_mtok: cache_write_5m_per_mtok,
                tiers,
            },
            scope,
        }
    }
}

/// Resolver backed by the embedded LiteLLM public pricing table.
pub struct StaticPricingResolver {
    table: HashMap<ModelKey, ResolvedPricing>,
}

impl StaticPricingResolver {
    /// Build from the embedded `litellm_public.json`.
    pub fn from_embedded() -> Self {
        let json = include_str!(concat!(env!("OUT_DIR"), "/litellm_public.json"));
        Self::from_json(json)
    }

    /// Empty resolver (used as the custom slot when no storage is available).
    pub fn empty() -> Self {
        Self {
            table: HashMap::new(),
        }
    }

    fn from_json(json: &str) -> Self {
        let mut parsed: HashMap<String, LiteLlmModel> =
            serde_json::from_str(json).expect("embedded litellm_public.json must be valid");
        // `sample_spec` documents the schema, not a priced model. Drop it
        // so lookups for it yield `None`, matching the metadata resolver.
        parsed.remove("sample_spec");
        let mut table = HashMap::new();
        for (model, entry) in parsed {
            // Public table is keyed by model name only.
            table.insert(ModelKey::new(None, &model), entry.to_resolved());
        }
        Self { table }
    }
}

impl PricingResolver for StaticPricingResolver {
    fn resolve(&self, provider: Option<&str>, model: &str) -> Option<ResolvedPricing> {
        // Try (provider, model) first, then model-only.
        self.table
            .get(&ModelKey::new(provider, model))
            .or_else(|| self.table.get(&ModelKey::new(None, model)))
            .cloned()
    }
}
