//! Favorites (stars, pins, tags) over arbitrary resources.
//!
//! Stored as a single snapshot document (`web:favorites`, a JSON array), so
//! listing needs no storage migration. Entries are small by construction;
//! list results are filtered and paginated in memory.

use serde::{Deserialize, Serialize};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::{now, timestamp_to_iso};

/// Persistence key holding the favorites array.
const FAVORITES_KEY: &str = "web:favorites";

/// Maximum favorites per server; bounds the single snapshot document.
const MAX_ENTRIES: usize = 2048;

/// Maximum tags per entry.
const MAX_TAGS: usize = 32;

/// Maximum tag length.
const MAX_TAG_LEN: usize = 64;

/// Maximum kind/id segment length.
const MAX_SEGMENT_LEN: usize = 192;

/// A single favorite: a pinned or tagged pointer at a resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoriteEntry {
    pub kind: String,
    pub id: String,
    pub pinned: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: String,
}

fn validate_segment(name: &str, value: &str) -> ApiResult<()> {
    if value.is_empty() || value.len() > MAX_SEGMENT_LEN {
        return Err(ApiError::Validation(format!(
            "{name} must be 1..={MAX_SEGMENT_LEN} chars"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::Validation(format!(
            "{name} contains control characters"
        )));
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> ApiResult<()> {
    if tags.len() > MAX_TAGS {
        return Err(ApiError::Validation(format!(
            "at most {MAX_TAGS} tags per favorite"
        )));
    }
    for tag in tags {
        if tag.is_empty() || tag.len() > MAX_TAG_LEN {
            return Err(ApiError::Validation(format!(
                "tags must be 1..={MAX_TAG_LEN} chars"
            )));
        }
    }
    Ok(())
}

async fn load_all(ctx: &ApiContext) -> ApiResult<Vec<FavoriteEntry>> {
    let stored = ctx.persistence.load_snapshot(FAVORITES_KEY).await?;
    match stored {
        None => Ok(Vec::new()),
        Some(value) => Ok(serde_json::from_value::<Vec<FavoriteEntry>>(value)
            .map_err(|_| ApiError::Validation("favorites document is corrupt".to_string()))?),
    }
}

async fn store_all(ctx: &ApiContext, entries: &[FavoriteEntry]) -> ApiResult<()> {
    if entries.len() > MAX_ENTRIES {
        return Err(ApiError::Validation(format!(
            "favorites exceed {MAX_ENTRIES} entries"
        )));
    }
    ctx.persistence
        .save_snapshot(FAVORITES_KEY, &serde_json::to_value(entries)?)
        .await?;
    Ok(())
}

/// List favorites, newest first, with cursor-style pagination. `limit` is
/// capped; `has_more` tells the caller whether another page exists.
pub async fn list(
    ctx: &ApiContext,
    kind: Option<&str>,
    pinned_only: bool,
    limit: u64,
    offset: u64,
) -> ApiResult<(Vec<FavoriteEntry>, bool)> {
    let mut entries = load_all(ctx).await?;
    entries.retain(|e| kind.is_none_or(|k| e.kind == k) && (!pinned_only || e.pinned));
    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let offset = offset as usize;
    let limit = limit.clamp(1, 500) as usize;
    let has_more = entries.len() > offset.saturating_add(limit);
    let page = entries.into_iter().skip(offset).take(limit).collect();
    Ok((page, has_more))
}

/// Insert or update a favorite. `pinned` / `tags` replace the stored value
/// when given and keep it when omitted.
pub async fn upsert(
    ctx: &ApiContext,
    kind: &str,
    id: &str,
    pinned: Option<bool>,
    tags: Option<Vec<String>>,
) -> ApiResult<FavoriteEntry> {
    validate_segment("kind", kind)?;
    validate_segment("id", id)?;
    if let Some(tags) = &tags {
        validate_tags(tags)?;
    }
    let mut entries = load_all(ctx).await?;
    let position = entries.iter().position(|e| e.kind == kind && e.id == id);
    let mut entry = position
        .map(|i| entries.remove(i))
        .unwrap_or(FavoriteEntry {
            kind: kind.to_string(),
            id: id.to_string(),
            pinned: false,
            tags: Vec::new(),
            updated_at: timestamp_to_iso(now()),
        });
    if let Some(pinned) = pinned {
        entry.pinned = pinned;
    }
    if let Some(tags) = tags {
        entry.tags = tags;
    }
    entry.updated_at = timestamp_to_iso(now());
    entries.push(entry.clone());
    store_all(ctx, &entries).await?;
    Ok(entry)
}

/// Remove a favorite. Returns whether an entry was removed.
pub async fn remove(ctx: &ApiContext, kind: &str, id: &str) -> ApiResult<bool> {
    validate_segment("kind", kind)?;
    validate_segment("id", id)?;
    let mut entries = load_all(ctx).await?;
    let before = entries.len();
    entries.retain(|e| !(e.kind == kind && e.id == id));
    if entries.len() == before {
        return Ok(false);
    }
    store_all(ctx, &entries).await?;
    Ok(true)
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
    async fn upsert_list_remove() {
        let ctx = ctx();
        let (page, more) = list(&ctx, None, false, 50, 0).await.unwrap();
        assert!(page.is_empty() && !more);
        upsert(&ctx, "execution", "e1", Some(true), Some(vec!["t".into()]))
            .await
            .unwrap();
        upsert(&ctx, "workflow", "w1", None, None).await.unwrap();
        let (all, _) = list(&ctx, None, false, 50, 0).await.unwrap();
        assert_eq!(all.len(), 2);
        let (pinned, _) = list(&ctx, None, true, 50, 0).await.unwrap();
        assert_eq!(pinned.len(), 1);
        let (execs, _) = list(&ctx, Some("execution"), false, 50, 0).await.unwrap();
        assert_eq!(execs.len(), 1);
        assert!(remove(&ctx, "execution", "e1").await.unwrap());
        assert!(!remove(&ctx, "execution", "e1").await.unwrap());
    }

    #[tokio::test]
    async fn invalid_segments_rejected() {
        let ctx = ctx();
        assert!(upsert(&ctx, "", "e1", None, None).await.is_err());
        assert!(upsert(&ctx, "k", "", None, None).await.is_err());
    }
}
