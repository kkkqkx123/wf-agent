//! Event filtering by query options.

use wf_types::events::BaseEvent;

use crate::infra::subscription::EventSubscriptionOptions;

use super::EventQueryOptions;

/// Apply `options` to a set of events, honoring the limit.
pub(crate) fn filter_events(
    events: Vec<BaseEvent>,
    options: &EventQueryOptions,
) -> Vec<BaseEvent> {
    let limit = options.effective_limit();
    let filter = EventSubscriptionOptions {
        execution_id: options.execution_id.clone(),
        agent_loop_id: options.agent_loop_id.clone(),
        workflow_id: options.workflow_id.clone(),
        event_types: options.event_types.clone(),
    };
    let mut out = Vec::new();
    for event in events {
        if !filter.matches(&event) {
            continue;
        }
        out.push(event);
        if out.len() >= limit {
            break;
        }
    }
    out
}
