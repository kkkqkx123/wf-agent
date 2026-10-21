//! Resolver composition and shared lookup key.

use std::sync::Arc;

/// Re-export of the resolver trait defined in `llm-suite-token`, so host code
/// can name it through `wf_pricing::PricingResolver` without depending on the
/// submodule directly.
pub use llm_token::PricingResolver;
pub use llm_token::ResolvedPricing;

/// Lookup key for a pricing rule: an optional provider plus a model name.
///
/// Resolution tries the `(provider, model)` pair first and falls back to a
/// model-only lookup, so a rule can apply across providers when no provider is
/// specified.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelKey {
    pub provider: Option<String>,
    pub model: String,
}

impl ModelKey {
    pub fn new(provider: Option<&str>, model: &str) -> Self {
        Self {
            provider: provider.map(str::to_string),
            model: model.to_string(),
        }
    }
}

/// Composes a custom (higher-priority) resolver over a public fallback.
pub struct CompositePricingResolver {
    custom: Arc<dyn PricingResolver>,
    public: Arc<dyn PricingResolver>,
}

impl CompositePricingResolver {
    /// `custom` is consulted first; `public` is the fallback.
    pub fn new(custom: Arc<dyn PricingResolver>, public: Arc<dyn PricingResolver>) -> Self {
        Self { custom, public }
    }
}

impl PricingResolver for CompositePricingResolver {
    fn resolve(&self, provider: Option<&str>, model: &str) -> Option<ResolvedPricing> {
        self.custom
            .resolve(provider, model)
            .or_else(|| self.public.resolve(provider, model))
    }
}
