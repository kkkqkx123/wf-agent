//! Pricing resolver translating local provider ids to LiteLLM namespaces.
//!
//! The embedded LiteLLM table stores provider-specific variants under keys
//! such as `azure/gpt-4o`, while local provider ids (`builtin-azure`) are
//! not valid LiteLLM namespaces. This wrapper maps local ids first and
//! tries the provider-qualified model key before falling back to the
//! canonical model-only entry. Storage-backed custom rules always win over
//! the public table, so explicit configuration stays authoritative.

use std::collections::HashMap;
use std::sync::Arc;

use crate::resolver::PricingResolver;
use llm_token::ResolvedPricing;

/// Maps `(provider, model)` to pricing with LiteLLM namespace translation.
///
/// Holds the custom and public resolvers separately (rather than wrapping
/// an already-composed resolver) so every custom rule — local, qualified
/// or canonical — is consulted before any public entry.
pub struct ProviderMappedPricingResolver {
    custom: Arc<dyn PricingResolver>,
    public: Arc<dyn PricingResolver>,
    mapping: HashMap<String, String>,
}

impl ProviderMappedPricingResolver {
    /// `mapping` translates local provider ids to LiteLLM namespaces.
    /// Providers absent from it resolve through the canonical fallback.
    pub fn new(
        custom: Arc<dyn PricingResolver>,
        public: Arc<dyn PricingResolver>,
        mapping: HashMap<String, String>,
    ) -> Self {
        Self {
            custom,
            public,
            mapping,
        }
    }

    /// The active local-id to LiteLLM-namespace translation table.
    pub fn mapping(&self) -> &HashMap<String, String> {
        &self.mapping
    }
}

impl PricingResolver for ProviderMappedPricingResolver {
    fn resolve(&self, provider: Option<&str>, model: &str) -> Option<ResolvedPricing> {
        let qualified = provider
            .and_then(|id| self.mapping.get(id))
            .filter(|_| !model.contains('/'))
            .map(|namespace| format!("{namespace}/{model}"));

        // Custom rules first: a qualified custom rule, then the local and
        // canonical custom rules. Each `resolve` on the qualified key is
        // exact (the table keys it verbatim), so no public entry can win
        // over an explicit custom rule here.
        if let Some(key) = qualified.as_deref() {
            if let Some(resolved) = self.custom.resolve(None, key) {
                return Some(resolved);
            }
        }
        if let Some(resolved) = self.custom.resolve(provider, model) {
            return Some(resolved);
        }

        // Public fallback: provider-specific qualified entry, then an
        // already-qualified model name as-is, then the canonical entry
        // (the public resolver itself falls back from provider to
        // model-only, so the canonical case is covered twice over).
        if let Some(key) = qualified.as_deref() {
            if let Some(resolved) = self.public.resolve(None, key) {
                return Some(resolved);
            }
        }
        if model.contains('/') {
            if let Some(resolved) = self.public.resolve(None, model) {
                return Some(resolved);
            }
        }
        self.public.resolve(provider, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_table::StaticPricingResolver;

    struct FixedResolver {
        provider: Option<String>,
        model: String,
        input_per_mtok: f64,
    }

    impl PricingResolver for FixedResolver {
        fn resolve(&self, provider: Option<&str>, model: &str) -> Option<ResolvedPricing> {
            let provider_match = match (&self.provider, provider) {
                (None, None) => true,
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if provider_match && self.model == model {
                Some(ResolvedPricing {
                    pricing: llm_token::TokenPricing {
                        input_per_mtok: self.input_per_mtok,
                        output_per_mtok: 0.0,
                        cache_read_per_mtok: None,
                        cache_write_5m_per_mtok: None,
                        cache_write_1h_per_mtok: None,
                        tiers: None,
                    },
                    scope: llm_token::CacheTokenScope::IncludedInPrompt,
                })
            } else {
                None
            }
        }
    }

    fn mapped(
        custom: Arc<dyn PricingResolver>,
        public: Arc<dyn PricingResolver>,
    ) -> ProviderMappedPricingResolver {
        let mapping = HashMap::from([("builtin-azure".to_string(), "azure".to_string())]);
        ProviderMappedPricingResolver::new(custom, public, mapping)
    }

    #[test]
    fn custom_rule_beats_public_qualified_entry() {
        let custom: Arc<dyn PricingResolver> = Arc::new(FixedResolver {
            provider: Some("builtin-azure".to_string()),
            model: "gpt-4o".to_string(),
            input_per_mtok: 999.0,
        });
        let public: Arc<dyn PricingResolver> = Arc::new(StaticPricingResolver::from_embedded());
        let resolver = mapped(custom, public);
        let resolved = resolver
            .resolve(Some("builtin-azure"), "gpt-4o")
            .expect("custom price");
        assert_eq!(resolved.pricing.input_per_mtok, 999.0);
    }

    #[test]
    fn qualified_public_entry_preferred_over_canonical() {
        let custom: Arc<dyn PricingResolver> = Arc::new(StaticPricingResolver::empty());
        let public: Arc<dyn PricingResolver> = Arc::new(StaticPricingResolver::from_embedded());
        let resolver = mapped(custom, Arc::clone(&public));
        let via_mapping = resolver
            .resolve(Some("builtin-azure"), "gpt-4o")
            .expect("azure price");
        let canonical = public.resolve(None, "gpt-4o").expect("canonical");
        assert_eq!(
            via_mapping.pricing.input_per_mtok,
            canonical.pricing.input_per_mtok
        );
        // The qualified key itself resolves directly.
        assert!(resolver.resolve(None, "azure/gpt-4o").is_some());
    }

    #[test]
    fn unmapped_provider_falls_back_to_canonical() {
        let custom: Arc<dyn PricingResolver> = Arc::new(StaticPricingResolver::empty());
        let public: Arc<dyn PricingResolver> = Arc::new(StaticPricingResolver::from_embedded());
        let resolver = mapped(custom, Arc::clone(&public));
        let resolved = resolver
            .resolve(Some("builtin-unknown"), "gpt-4o")
            .expect("canonical fallback");
        let canonical = public.resolve(None, "gpt-4o").expect("canonical");
        assert_eq!(
            resolved.pricing.input_per_mtok,
            canonical.pricing.input_per_mtok
        );
        assert!(resolver.resolve(None, "no-such-model-xyz").is_none());
    }
}
