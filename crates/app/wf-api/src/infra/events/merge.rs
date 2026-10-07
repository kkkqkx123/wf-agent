//! Merging of the bounded bus window with persisted events.

use wf_types::events::BaseEvent;

use crate::infra::context::ApiContext;

use super::EventQueryOptions;

/// Merge persisted events with the bounded bus window, deduplicated by id.
pub(crate) async fn merge(ctx: &ApiContext, options: &EventQueryOptions) -> Vec<BaseEvent> {
    let mut events: Vec<BaseEvent> = ctx.event_bus.recent_events();
    if let Ok(persisted) = ctx.persistence.query_events(options).await {
        events.extend(persisted);
    }
    events.sort_by_key(|e| e.timestamp);
    events.dedup_by_key(|e| e.id.clone());
    events
}
