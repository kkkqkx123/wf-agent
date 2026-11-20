//! Runtime-overridable retention settings.
//!
//! The infrastructure file supplies the bootstrap default; the typed settings
//! API persists explicit changes in a server-owned snapshot so the web
//! preferences document never controls destructive behavior.

use wf_types::config::RetentionConfig;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

const RETENTION_KEY: &str = "system:retention";

/// Effective policy: persisted override when present, bootstrap otherwise.
pub async fn current(ctx: &ApiContext) -> ApiResult<RetentionConfig> {
    let Some(value) = ctx.persistence.load_snapshot(RETENTION_KEY).await? else {
        return Ok(ctx.retention.clone().unwrap_or_default());
    };
    serde_json::from_value(value)
        .map_err(|e| ApiError::Validation(format!("retention settings document is corrupt: {e}")))
}

/// Replace the runtime override after validating the complete document.
pub async fn replace(ctx: &ApiContext, config: RetentionConfig) -> ApiResult<RetentionConfig> {
    wf_config::processor::retention::validate_retention_config(&config)
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    let value = serde_json::to_value(&config)?;
    ctx.persistence.save_snapshot(RETENTION_KEY, &value).await?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn ctx() -> ApiContext {
        ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        )
    }

    #[tokio::test]
    async fn persisted_settings_override_the_bootstrap_default() {
        let ctx = ctx();
        assert_eq!(current(&ctx).await.unwrap(), RetentionConfig::default());

        let next = RetentionConfig {
            enabled: true,
            dry_run: false,
            ..Default::default()
        };
        replace(&ctx, next.clone()).await.unwrap();
        assert_eq!(current(&ctx).await.unwrap(), next);
    }
}
