//! Resolver backed by the `pricing_rule` store in `wf-storage`.
//!
//! The store is loaded into an in-memory mirror at construction (and can be
//! reloaded via [`StoragePricingResolver::refresh`]); resolution is then a
//! synchronous `HashMap` lookup on the metrics hot path, so it never blocks on
//! the storage backend.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::resolver::{ModelKey, PricingResolver};
use llm_token::{CacheTokenScope, ResolvedPricing, TokenPricing};
use wf_storage::adapter::{BaseStorageAdapter, PricingRule, PricingScope};
use wf_storage::context::StorageContext;

/// In-memory mirror of the `pricing_rule` store, consulted synchronously on the
/// metrics hot path.
pub struct StoragePricingResolver {
    mirror: Mutex<HashMap<ModelKey, ResolvedPricing>>,
}

impl StoragePricingResolver {
    /// Load every `PricingRule` from the storage context into the mirror.
    pub async fn from_context(ctx: &StorageContext) -> Self {
        let mut mirror = HashMap::new();
        if let Ok(rules) = ctx.pricing_rule.list(None).await {
            for rule in rules {
                mirror.insert(
                    ModelKey::new(rule.provider.as_deref(), &rule.model),
                    to_resolved(&rule),
                );
            }
        }
        Self {
            mirror: Mutex::new(mirror),
        }
    }

    /// Reload the mirror from the store (call after custom rules change).
    pub async fn refresh(&self, ctx: &StorageContext) {
        let mut mirror = HashMap::new();
        if let Ok(rules) = ctx.pricing_rule.list(None).await {
            for rule in rules {
                mirror.insert(
                    ModelKey::new(rule.provider.as_deref(), &rule.model),
                    to_resolved(&rule),
                );
            }
        }
        *self.mirror.lock().expect("pricing mirror poisoned") = mirror;
    }
}

fn to_resolved(rule: &PricingRule) -> ResolvedPricing {
    let scope = match rule.scope {
        PricingScope::IncludedInPrompt => CacheTokenScope::IncludedInPrompt,
        PricingScope::SeparateFromPrompt => CacheTokenScope::SeparateFromPrompt,
    };
    ResolvedPricing {
        pricing: TokenPricing {
            input_per_mtok: rule.input_per_mtok,
            output_per_mtok: rule.output_per_mtok,
            cache_read_per_mtok: rule.cache_read_per_mtok,
            cache_write_5m_per_mtok: rule.cache_write_5m_per_mtok,
            cache_write_1h_per_mtok: rule.cache_write_1h_per_mtok,
            tiers: None,
        },
        scope,
    }
}

impl PricingResolver for StoragePricingResolver {
    fn resolve(&self, provider: Option<&str>, model: &str) -> Option<ResolvedPricing> {
        let mirror = self.mirror.lock().expect("pricing mirror poisoned");
        mirror
            .get(&ModelKey::new(provider, model))
            .or_else(|| mirror.get(&ModelKey::new(None, model)))
            .cloned()
    }
}
