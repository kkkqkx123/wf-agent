//! Event history / timeline queries over the merged event store (bus window
//! + persisted events).

use std::time::Duration;

use wf_types::events::BaseEvent;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::subscription::{
    spawn_event_subscription, EventSubscription, EventSubscriptionOptions,
};

use super::filter::filter_events;
use super::merge::merge;
use super::EventQueryOptions;

/// Recent events matching the query options, newest first.
pub async fn history(ctx: &ApiContext, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
    let events = merge(ctx, options).await;
    let mut events = filter_events(events, options);
    events.reverse();
    Ok(events)
}

/// Event timeline of a single workflow execution, oldest first.
pub async fn timeline(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        execution_id: Some(execution_id.to_string()),
        limit: None,
        ..Default::default()
    };
    let mut events = filter_events(merge(ctx, &options).await, &options);
    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

/// Event timeline of a single agent loop, oldest first.
pub async fn agent_timeline(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        agent_loop_id: Some(agent_loop_id.to_string()),
        limit: None,
        ..Default::default()
    };
    let mut events = filter_events(merge(ctx, &options).await, &options);
    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

/// Subscribe to matching events, delivered as an async stream.
pub fn subscribe(ctx: &ApiContext, options: EventSubscriptionOptions) -> EventSubscription {
    spawn_event_subscription(ctx.event_bus.clone(), &options)
}

/// Await the first matching event, bounded by `timeout`.
pub async fn wait_for_event(
    ctx: &ApiContext,
    options: EventSubscriptionOptions,
    timeout: Duration,
) -> ApiResult<Option<BaseEvent>> {
    crate::infra::subscription::wait_for_event(ctx.event_bus.clone(), &options, timeout).await
}
