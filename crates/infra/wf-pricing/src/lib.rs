//! LiteLLM-style pricing resolution for LLM token cost.
//!
//! Provides a composed resolver that consults a storage-backed **custom** table
//! first and falls back to an embedded **public** LiteLLM table. The resolver
//! trait itself lives in `llm-suite-token` (so the gateway in the submodule can
//! depend on it without knowing about this host crate); this crate supplies the
//! concrete implementations and the runtime factory.

pub mod resolver;
pub mod static_table;
pub mod storage_table;

pub use resolver::{CompositePricingResolver, PricingResolver};
pub use static_table::StaticPricingResolver;
pub use storage_table::StoragePricingResolver;

use std::sync::Arc;

use wf_storage::context::StorageContext;

/// Build the composed resolver used at runtime.
///
/// Resolution priority: the storage-backed custom table wins, the embedded
/// public LiteLLM table is the fallback. When `storage` is `None` (e.g. the
/// storage manager is unavailable) only the public table is used.
pub async fn build_composite_resolver(
    storage: Option<&StorageContext>,
) -> Arc<dyn llm_token::PricingResolver> {
    let public = Arc::new(StaticPricingResolver::from_embedded());
    let custom: Arc<dyn llm_token::PricingResolver> = match storage {
        Some(ctx) => Arc::new(StoragePricingResolver::from_context(ctx).await),
        None => Arc::new(StaticPricingResolver::empty()),
    };
    Arc::new(CompositePricingResolver::new(custom, public))
}
