//! UI preference document: an open JSON object owned by the web frontend.
//!
//! The server stores the document verbatim and validates only the envelope
//! (key shape, entry count, serialized size). Preference semantics stay in
//! the frontend; see `docs/spec/web/style-guide.md`.

use serde_json::{Map, Value};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Persistence key holding the whole preference document.
const PREFS_KEY: &str = "web:preferences";

/// Maximum entries per document; bounds a single snapshot write.
const MAX_ENTRIES: usize = 512;

/// Maximum serialized document size in bytes.
const MAX_DOCUMENT_BYTES: usize = 262_144;

/// Maximum preference key length.
const MAX_KEY_LEN: usize = 128;

/// Preference keys are frontend-owned dotted paths (`theme`, `sidebar.width`).
fn validate_key(key: &str) -> ApiResult<()> {
    if key.is_empty() || key.len() > MAX_KEY_LEN {
        return Err(ApiError::Validation(format!(
            "preference key must be 1..={MAX_KEY_LEN} chars"
        )));
    }
    let valid = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/');
    if !valid {
        return Err(ApiError::Validation(format!(
            "preference key [{key}] uses unsupported characters"
        )));
    }
    Ok(())
}

async fn load_document(ctx: &ApiContext) -> ApiResult<Map<String, Value>> {
    let stored = ctx.persistence.load_snapshot(PREFS_KEY).await?;
    match stored {
        None => Ok(Map::new()),
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err(ApiError::Validation(
            "preference document is corrupt (expected a JSON object)".to_string(),
        )),
    }
}

async fn store_document(ctx: &ApiContext, doc: &Map<String, Value>) -> ApiResult<()> {
    if doc.len() > MAX_ENTRIES {
        return Err(ApiError::Validation(format!(
            "preference document exceeds {MAX_ENTRIES} entries"
        )));
    }
    let value = Value::Object(doc.clone());
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(ApiError::Validation(format!(
            "preference document exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    ctx.persistence.save_snapshot(PREFS_KEY, &value).await?;
    Ok(())
}

/// Load the whole preference document (empty object when unset).
pub async fn get_all(ctx: &ApiContext) -> ApiResult<Map<String, Value>> {
    load_document(ctx).await
}

/// Replace the whole preference document.
pub async fn replace_all(
    ctx: &ApiContext,
    values: Map<String, Value>,
) -> ApiResult<Map<String, Value>> {
    for key in values.keys() {
        validate_key(key)?;
    }
    store_document(ctx, &values).await?;
    Ok(values)
}

/// Read a single preference by key.
pub async fn get_key(ctx: &ApiContext, key: &str) -> ApiResult<Option<Value>> {
    validate_key(key)?;
    Ok(load_document(ctx).await?.remove(key))
}

/// Insert or overwrite a single preference.
pub async fn set_key(ctx: &ApiContext, key: &str, value: Value) -> ApiResult<Value> {
    validate_key(key)?;
    let mut doc = load_document(ctx).await?;
    doc.insert(key.to_string(), value.clone());
    store_document(ctx, &doc).await?;
    Ok(value)
}

/// Delete a single preference. Returns whether a value was removed.
pub async fn delete_key(ctx: &ApiContext, key: &str) -> ApiResult<bool> {
    validate_key(key)?;
    let mut doc = load_document(ctx).await?;
    let removed = doc.remove(key).is_some();
    if removed {
        store_document(ctx, &doc).await?;
    }
    Ok(removed)
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
    async fn round_trip_single_key() {
        let ctx = ctx();
        assert_eq!(get_key(&ctx, "theme").await.unwrap(), None);
        set_key(&ctx, "theme", Value::String("dark".into()))
            .await
            .unwrap();
        assert_eq!(
            get_key(&ctx, "theme").await.unwrap(),
            Some(Value::String("dark".into()))
        );
        assert!(delete_key(&ctx, "theme").await.unwrap());
        assert!(!delete_key(&ctx, "theme").await.unwrap());
    }

    #[tokio::test]
    async fn replace_all_replaces() {
        let ctx = ctx();
        set_key(&ctx, "a", Value::from(1)).await.unwrap();
        let mut next = Map::new();
        next.insert("b".to_string(), Value::from(2));
        replace_all(&ctx, next).await.unwrap();
        let doc = get_all(&ctx).await.unwrap();
        assert!(doc.get("a").is_none());
        assert_eq!(doc.get("b"), Some(&Value::from(2)));
    }

    #[tokio::test]
    async fn invalid_keys_rejected() {
        let ctx = ctx();
        assert!(get_key(&ctx, "").await.is_err());
        assert!(get_key(&ctx, "has space").await.is_err());
        assert!(set_key(&ctx, "ok", Value::Null).await.is_ok());
    }
}
