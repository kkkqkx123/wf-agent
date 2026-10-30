//! Chat starter prompts: the suggestion chips of an empty chat session.
//!
//! A starter fills the user's draft (`OutgoingMessage.text`); it never
//! touches the agent system prompt (`system_prompt` /
//! `system_prompt_template_id`), which stays owned by the prompt-template
//! registry in `wf-resource`. Builtins ship as read-only seed rows
//! (`is_system`); user rows persist in the `chat:starters` snapshot
//! alongside the other web-facing support state, so no storage migration
//! is needed.

use serde::{Deserialize, Serialize};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::{now, timestamp_to_iso};

/// Persistence key holding the whole starter list (builtins + customs).
const STARTERS_KEY: &str = "chat:starters";

/// Maximum starters per server; bounds the single snapshot document.
const MAX_STARTERS: usize = 512;
/// Maximum title / content / description lengths.
const MAX_TITLE_LEN: usize = 200;
const MAX_CONTENT_LEN: usize = 4000;
const MAX_DESC_LEN: usize = 500;
/// Maximum tags per starter and tag length.
const MAX_TAGS: usize = 16;
const MAX_TAG_LEN: usize = 64;
/// Maximum id / locale lengths.
const MAX_ID_LEN: usize = 192;
const MAX_LOCALE_LEN: usize = 16;

/// One chat starter: a suggestion chip filling the composer draft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StarterEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// BCP-47-ish locale of the row (`en`, `zh`); customs carry the locale
    /// they were authored in.
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether the row belongs to the default empty-state suggestion set.
    #[serde(default)]
    pub is_default: bool,
    /// Builtin seed rows are read-only (no edit / delete through the API).
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub usage_count: u64,
    pub created_at: String,
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

fn default_locale() -> String {
    "en".to_string()
}

/// Input for creating a custom starter. `id` is optional; an empty id is
/// generated server-side. System flags and usage are never client-set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStarterInput {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

/// Input for updating a custom starter. Every field replaces the stored
/// value when present and keeps it when omitted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateStarterInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

fn seed_row(id: &str, title: &str, content: &str, locale: &str, stamp: &str) -> StarterEntry {
    StarterEntry {
        id: id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        description: None,
        locale: locale.to_string(),
        tags: vec!["general".to_string()],
        enabled: true,
        is_default: true,
        is_system: true,
        usage_count: 0,
        created_at: stamp.to_string(),
        updated_at: stamp.to_string(),
    }
}

/// Builtin seed rows: three suggestion concepts in two locales. Ids are
/// stable across restarts; text refreshes on load so copy edits propagate
/// while per-row `enabled` / `usage_count` stay user-owned.
fn builtin_starters() -> Vec<StarterEntry> {
    let stamp = timestamp_to_iso(now());
    vec![
        seed_row(
            "starter.summary-en",
            "Summarize the current workspace state",
            "Summarize the current workspace state",
            "en",
            &stamp,
        ),
        seed_row(
            "starter.tools-en",
            "List the available tools and skills",
            "List the available tools and skills",
            "en",
            &stamp,
        ),
        seed_row(
            "starter.plan-en",
            "Draft a plan before making changes",
            "Draft a plan before making changes",
            "en",
            &stamp,
        ),
        seed_row(
            "starter.summary-zh",
            "总结当前工作区状态",
            "总结当前工作区状态",
            "zh",
            &stamp,
        ),
        seed_row(
            "starter.tools-zh",
            "列出可用的工具和技能",
            "列出可用的工具和技能",
            "zh",
            &stamp,
        ),
        seed_row(
            "starter.plan-zh",
            "在动手前先拟定计划",
            "在动手前先拟定计划",
            "zh",
            &stamp,
        ),
    ]
}

fn validate_id(id: &str) -> ApiResult<()> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(ApiError::Validation(format!(
            "starter id must be 1..={MAX_ID_LEN} chars"
        )));
    }
    if id.chars().any(char::is_control) {
        return Err(ApiError::Validation(
            "starter id contains control characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_text(
    title: &str,
    content: &str,
    description: Option<&str>,
    locale: &str,
    tags: &[String],
) -> ApiResult<()> {
    if title.trim().is_empty() || title.len() > MAX_TITLE_LEN {
        return Err(ApiError::Validation(format!(
            "starter title must be 1..={MAX_TITLE_LEN} chars"
        )));
    }
    if content.trim().is_empty() || content.len() > MAX_CONTENT_LEN {
        return Err(ApiError::Validation(format!(
            "starter content must be 1..={MAX_CONTENT_LEN} chars"
        )));
    }
    if let Some(description) = description {
        if description.len() > MAX_DESC_LEN {
            return Err(ApiError::Validation(format!(
                "starter description must be 0..={MAX_DESC_LEN} chars"
            )));
        }
    }
    if locale.is_empty() || locale.len() > MAX_LOCALE_LEN {
        return Err(ApiError::Validation(format!(
            "starter locale must be 1..={MAX_LOCALE_LEN} chars"
        )));
    }
    if tags.len() > MAX_TAGS {
        return Err(ApiError::Validation(format!(
            "at most {MAX_TAGS} tags per starter"
        )));
    }
    for tag in tags {
        if tag.trim().is_empty() || tag.len() > MAX_TAG_LEN {
            return Err(ApiError::Validation(format!(
                "starter tags must be 1..={MAX_TAG_LEN} chars"
            )));
        }
    }
    Ok(())
}

async fn load_all(ctx: &ApiContext) -> ApiResult<Vec<StarterEntry>> {
    let stored = ctx.persistence.load_snapshot(STARTERS_KEY).await?;
    match stored {
        None => {
            let seeded = builtin_starters();
            store_all(ctx, &seeded).await?;
            Ok(seeded)
        }
        Some(value) => {
            let mut entries = serde_json::from_value::<Vec<StarterEntry>>(value)
                .map_err(|_| ApiError::Validation("starter document is corrupt".to_string()))?;
            // Refresh builtin text on every load: copy edits propagate
            // while `enabled` / `usage_count` stay user-owned.
            let mut changed = false;
            for seed in builtin_starters() {
                match entries.iter_mut().find(|e| e.id == seed.id) {
                    None => {
                        entries.push(seed);
                        changed = true;
                    }
                    Some(existing) => {
                        if existing.title != seed.title
                            || existing.content != seed.content
                            || existing.locale != seed.locale
                        {
                            existing.title = seed.title;
                            existing.content = seed.content;
                            existing.locale = seed.locale.clone();
                            existing.updated_at = seed.updated_at.clone();
                            changed = true;
                        }
                        existing.is_system = true;
                    }
                }
            }
            if changed {
                store_all(ctx, &entries).await?;
            }
            Ok(entries)
        }
    }
}

async fn store_all(ctx: &ApiContext, entries: &[StarterEntry]) -> ApiResult<()> {
    if entries.len() > MAX_STARTERS {
        return Err(ApiError::Validation(format!(
            "starters exceed {MAX_STARTERS} entries"
        )));
    }
    ctx.persistence
        .save_snapshot(STARTERS_KEY, &serde_json::to_value(entries)?)
        .await?;
    Ok(())
}

fn sort_starters(entries: &mut [StarterEntry]) {
    entries.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| b.usage_count.cmp(&a.usage_count))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

/// List starters with chat / admin filtering. `locale` narrows to one
/// locale; `enabled_only` hides disabled rows (chat surface). `q` matches
/// title and content; `tags` requires at least one shared tag.
pub async fn list(
    ctx: &ApiContext,
    locale: Option<&str>,
    q: Option<&str>,
    tags: &[String],
    enabled_only: bool,
) -> ApiResult<Vec<StarterEntry>> {
    let mut entries = load_all(ctx).await?;
    if let Some(locale) = locale {
        let locale = locale.trim();
        if !locale.is_empty() {
            entries.retain(|e| e.locale == locale);
        }
    }
    if enabled_only {
        entries.retain(|e| e.enabled);
    }
    if let Some(q) = q {
        let q = q.trim().to_lowercase();
        if !q.is_empty() {
            entries.retain(|e| {
                e.title.to_lowercase().contains(&q) || e.content.to_lowercase().contains(&q)
            });
        }
    }
    if !tags.is_empty() {
        entries.retain(|e| e.tags.iter().any(|t| tags.contains(t)));
    }
    sort_starters(&mut entries);
    Ok(entries)
}

/// Fetch one starter by id.
pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<StarterEntry> {
    let entries = load_all(ctx).await?;
    entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| ApiError::not_found("starter", id))
}

/// Create a custom starter. Builtin ids are reserved.
pub async fn create(ctx: &ApiContext, input: CreateStarterInput) -> ApiResult<StarterEntry> {
    let id = match input.id {
        Some(id) => {
            let id = id.trim().to_string();
            validate_id(&id)?;
            id
        }
        None => format!("starter.custom.{}", crate::generate_id()),
    };
    let tags = input.tags.unwrap_or_default();
    let locale = input.locale.unwrap_or_else(default_locale);
    validate_text(
        &input.title,
        &input.content,
        input.description.as_deref(),
        &locale,
        &tags,
    )?;
    let mut entries = load_all(ctx).await?;
    if entries.iter().any(|e| e.id == id) {
        return Err(ApiError::already_exists("starter", &id));
    }
    let stamp = timestamp_to_iso(now());
    let entry = StarterEntry {
        id,
        title: input.title,
        content: input.content,
        description: input.description,
        locale,
        tags,
        enabled: input.enabled.unwrap_or(true),
        is_default: input.is_default.unwrap_or(false),
        is_system: false,
        usage_count: 0,
        created_at: stamp.clone(),
        updated_at: stamp,
    };
    entries.push(entry.clone());
    store_all(ctx, &entries).await?;
    Ok(entry)
}

/// Update a custom starter. Builtin rows reject edits.
pub async fn update(
    ctx: &ApiContext,
    id: &str,
    input: UpdateStarterInput,
) -> ApiResult<StarterEntry> {
    let mut entries = load_all(ctx).await?;
    let position = entries
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| ApiError::not_found("starter", id))?;
    if entries[position].is_system {
        return Err(ApiError::Validation(format!(
            "starter [{id}] is a builtin and cannot be edited"
        )));
    }
    {
        let entry = &mut entries[position];
        if let Some(title) = input.title {
            if title.trim().is_empty() || title.len() > MAX_TITLE_LEN {
                return Err(ApiError::Validation(format!(
                    "starter title must be 1..={MAX_TITLE_LEN} chars"
                )));
            }
            entry.title = title;
        }
        if let Some(content) = input.content {
            if content.trim().is_empty() || content.len() > MAX_CONTENT_LEN {
                return Err(ApiError::Validation(format!(
                    "starter content must be 1..={MAX_CONTENT_LEN} chars"
                )));
            }
            entry.content = content;
        }
        if let Some(description) = input.description {
            if let Some(description) = description.as_deref() {
                if description.len() > MAX_DESC_LEN {
                    return Err(ApiError::Validation(format!(
                        "starter description must be 0..={MAX_DESC_LEN} chars"
                    )));
                }
            }
            entry.description = description;
        }
        if let Some(locale) = input.locale {
            if locale.is_empty() || locale.len() > MAX_LOCALE_LEN {
                return Err(ApiError::Validation(format!(
                    "starter locale must be 1..={MAX_LOCALE_LEN} chars"
                )));
            }
            entry.locale = locale;
        }
        if let Some(tags) = input.tags {
            if tags.len() > MAX_TAGS
                || tags
                    .iter()
                    .any(|t| t.trim().is_empty() || t.len() > MAX_TAG_LEN)
            {
                return Err(ApiError::Validation("starter tags are invalid".to_string()));
            }
            entry.tags = tags;
        }
        if let Some(enabled) = input.enabled {
            entry.enabled = enabled;
        }
        if let Some(is_default) = input.is_default {
            entry.is_default = is_default;
        }
        entry.updated_at = timestamp_to_iso(now());
    }
    store_all(ctx, &entries).await?;
    Ok(entries[position].clone())
}

/// Delete a custom starter. Builtin rows are protected.
pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    let mut entries = load_all(ctx).await?;
    let position = entries
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| ApiError::not_found("starter", id))?;
    if entries[position].is_system {
        return Err(ApiError::Validation(format!(
            "starter [{id}] is a builtin and cannot be deleted"
        )));
    }
    entries.remove(position);
    store_all(ctx, &entries).await?;
    Ok(true)
}

/// Enable or disable a starter. Builtins accept the flag (so users can
/// hide them) but stay read-only for every other field.
pub async fn set_enabled(ctx: &ApiContext, id: &str, enabled: bool) -> ApiResult<StarterEntry> {
    let mut entries = load_all(ctx).await?;
    let position = entries
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| ApiError::not_found("starter", id))?;
    entries[position].enabled = enabled;
    entries[position].updated_at = timestamp_to_iso(now());
    store_all(ctx, &entries).await?;
    Ok(entries[position].clone())
}

/// Record one successful send of a starter. Click-to-fill does not count;
/// only an accepted send increments the counter.
pub async fn record_usage(ctx: &ApiContext, id: &str) -> ApiResult<u64> {
    let mut entries = load_all(ctx).await?;
    let position = entries
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| ApiError::not_found("starter", id))?;
    entries[position].usage_count = entries[position].usage_count.saturating_add(1);
    let count = entries[position].usage_count;
    store_all(ctx, &entries).await?;
    Ok(count)
}

/// Export one starter as a pretty JSON document for download / backup.
pub async fn export_json(ctx: &ApiContext, id: &str) -> ApiResult<String> {
    let entry = get(ctx, id).await?;
    serde_json::to_string_pretty(&entry).map_err(ApiError::from)
}

/// Import a starter document. Imported rows always land as customs under a
/// fresh id, even when the payload came from a builtin export.
pub async fn import_json(ctx: &ApiContext, raw: &str) -> ApiResult<StarterEntry> {
    let mut value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| ApiError::Validation(format!("invalid starter JSON: {e}")))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("id");
        object.remove("is_system");
        object.remove("usage_count");
        object.remove("created_at");
        object.remove("updated_at");
    }
    let input: CreateStarterInput = serde_json::from_value(value)
        .map_err(|e| ApiError::Validation(format!("invalid starter document: {e}")))?;
    create(ctx, input).await
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
    async fn first_list_seeds_builtins() {
        let ctx = ctx();
        let entries = list(&ctx, None, None, &[], false).await.unwrap();
        assert_eq!(entries.len(), 6);
        assert!(entries.iter().all(|e| e.is_system));
    }

    #[tokio::test]
    async fn locale_filter_splits_builtins() {
        let ctx = ctx();
        let en = list(&ctx, Some("en"), None, &[], true).await.unwrap();
        let zh = list(&ctx, Some("zh"), None, &[], true).await.unwrap();
        assert_eq!(en.len(), 3);
        assert_eq!(zh.len(), 3);
        assert!(en.iter().all(|e| e.locale == "en"));
        assert!(zh.iter().all(|e| e.locale == "zh"));
    }

    #[tokio::test]
    async fn builtin_text_refreshes_while_usage_stays() {
        let ctx = ctx();
        record_usage(&ctx, "starter.summary-en").await.unwrap();
        // Simulate an older copy: rewrite the snapshot with stale text and
        // a disabled flag, then reload and expect text refresh with the
        // user-owned fields preserved.
        let mut entries = load_all(&ctx).await.unwrap();
        let row = entries
            .iter_mut()
            .find(|e| e.id == "starter.summary-en")
            .unwrap();
        row.title = "stale".to_string();
        row.enabled = false;
        store_all(&ctx, &entries).await.unwrap();
        let refreshed = get(&ctx, "starter.summary-en").await.unwrap();
        assert_eq!(refreshed.title, "Summarize the current workspace state");
        assert_eq!(refreshed.usage_count, 1);
        assert!(!refreshed.enabled);
    }

    #[tokio::test]
    async fn custom_crud_roundtrip() {
        let ctx = ctx();
        let created = create(
            &ctx,
            CreateStarterInput {
                id: Some("starter.custom.demo".to_string()),
                title: "Demo".to_string(),
                content: "do the demo".to_string(),
                description: None,
                locale: Some("en".to_string()),
                tags: Some(vec!["demo".to_string()]),
                enabled: None,
                is_default: Some(true),
            },
        )
        .await
        .unwrap();
        assert!(!created.is_system);
        assert!(created.is_default);
        let updated = update(
            &ctx,
            "starter.custom.demo",
            UpdateStarterInput {
                title: Some("Demo v2".to_string()),
                ..UpdateStarterInput::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.title, "Demo v2");
        assert!(delete(&ctx, "starter.custom.demo").await.unwrap());
        assert!(get(&ctx, "starter.custom.demo").await.is_err());
    }

    #[tokio::test]
    async fn builtin_writes_rejected() {
        let ctx = ctx();
        assert!(update(
            &ctx,
            "starter.summary-en",
            UpdateStarterInput {
                title: Some("hijack".to_string()),
                ..UpdateStarterInput::default()
            },
        )
        .await
        .is_err());
        assert!(delete(&ctx, "starter.summary-en").await.is_err());
        // Enable / disable and usage stay allowed on builtins.
        set_enabled(&ctx, "starter.summary-en", false)
            .await
            .unwrap();
        let hidden = list(&ctx, Some("en"), None, &[], true).await.unwrap();
        assert_eq!(hidden.len(), 2);
        set_enabled(&ctx, "starter.summary-en", true).await.unwrap();
        assert_eq!(record_usage(&ctx, "starter.summary-en").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn invalid_inputs_rejected() {
        let ctx = ctx();
        assert!(create(
            &ctx,
            CreateStarterInput {
                id: Some("starter.custom.bad".to_string()),
                title: "  ".to_string(),
                content: "x".to_string(),
                description: None,
                locale: None,
                tags: None,
                enabled: None,
                is_default: None,
            },
        )
        .await
        .is_err());
        assert!(create(
            &ctx,
            CreateStarterInput {
                id: Some("starter.summary-en".to_string()),
                title: "clash".to_string(),
                content: "x".to_string(),
                description: None,
                locale: None,
                tags: None,
                enabled: None,
                is_default: None,
            },
        )
        .await
        .is_err());
        assert!(get(&ctx, "starter.missing").await.is_err());
        assert!(record_usage(&ctx, "starter.missing").await.is_err());
    }

    #[tokio::test]
    async fn query_and_tags_filter() {
        let ctx = ctx();
        create(
            &ctx,
            CreateStarterInput {
                id: Some("starter.custom.plan2".to_string()),
                title: "Release plan".to_string(),
                content: "plan the release".to_string(),
                description: None,
                locale: Some("en".to_string()),
                tags: Some(vec!["release".to_string()]),
                enabled: None,
                is_default: None,
            },
        )
        .await
        .unwrap();
        let hits = list(&ctx, Some("en"), Some("release"), &[], false)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        let tagged = list(&ctx, None, None, &["release".to_string()], false)
            .await
            .unwrap();
        assert_eq!(tagged.len(), 1);
    }

    #[tokio::test]
    async fn export_import_lands_as_custom() {
        let ctx = ctx();
        let raw = export_json(&ctx, "starter.plan-en").await.unwrap();
        let imported = import_json(&ctx, &raw).await.unwrap();
        assert!(!imported.is_system);
        assert_ne!(imported.id, "starter.plan-en");
        assert_eq!(imported.title, "Draft a plan before making changes");
    }
}
