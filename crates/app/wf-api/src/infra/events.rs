//! Event system API: durable dispatch, history/timeline queries, agent event
//! queries, listener statistics and health reporting.

mod agent_queries;
mod filter;
mod health;
mod merge;
mod timeline;
mod timeline_events;

use wf_types::events::{BaseEvent, EventType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

pub use agent_queries::{
    get_agent_events, get_agent_loop_statistics, get_agent_tool_execution_events,
    get_agent_turn_events, get_event_stats, search_events, stats, EventStats,
};
pub(crate) use filter::filter_events;
pub use health::{
    clear_event_history, event_history_size, event_system_health, event_time_range,
    execution_listener_stats, EventSystemHealth, ExecutionListenerStats,
};
pub use timeline::{
    execution_timeline_summary, get_execution_timeline, ExecutionTimeline,
    ExecutionTimelinePhase, ExecutionTimelineSummary,
};
pub use timeline_events::{agent_timeline, history, subscribe, timeline, wait_for_event};

/// Default maximum number of events returned when no explicit limit is given.
const DEFAULT_EVENT_LIMIT: usize = 100;

/// The most lifecycle events one history timeline read returns. A run that
/// recorded more has its later events left out, so the bound travels with the
/// response instead of being something a caller has to already know.
pub const TIMELINE_LIMIT: usize = DEFAULT_EVENT_LIMIT;

/// Query options for the event history / timeline endpoints.
#[derive(Debug, Clone, Default)]
pub struct EventQueryOptions {
    /// Only events belonging to this workflow execution.
    pub execution_id: Option<String>,
    /// Only events belonging to this agent loop.
    pub agent_loop_id: Option<String>,
    /// Only events referencing this workflow.
    pub workflow_id: Option<String>,
    /// Only events of these types; `None` returns every type.
    pub event_types: Option<Vec<EventType>>,
    /// Maximum number of events to return; `None` uses the default.
    pub limit: Option<usize>,
}

impl EventQueryOptions {
    fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_EVENT_LIMIT)
    }
}

/// Publish an event on the shared bus and persist it through the persistence
/// layer (durable event dispatch). Publishing with no active subscribers is not
/// an error — the event is already persisted.
pub async fn dispatch(ctx: &ApiContext, event: BaseEvent) -> ApiResult<()> {
    ctx.persistence.save_event(&event).await?;
    let _ = ctx.event_bus.publish(event);
    Ok(())
}

#[cfg(test)]
mod tests;
