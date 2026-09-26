//! Observability for fail-open data degradation.
//!
//! Where a parse failure lets the engine continue with degraded or empty
//! data, a log line alone hides "data silently dropped" in production. These
//! sites publish a subscribable `NODE_CUSTOM_EVENT` (metadata carries the
//! `data_degraded` marker, the failing site and the detail) so audits and
//! trigger templates can react to the loss itself.

use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

/// Event name carried in the custom-event metadata for degradation emits.
pub const DATA_DEGRADED_EVENT: &str = "data_degraded";

/// Publish a degradation event on the execution's event bus (no-op without
/// a bus; emission failures are logged, never propagated).
pub fn emit_data_degradation(
    event_bus: Option<&EventBus>,
    workflow_id: Option<Id>,
    execution_id: &Id,
    site: &str,
    detail: &str,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(
            execution_id = %execution_id,
            site,
            "no event bus, skipping data-degradation event"
        );
        return;
    };
    let metadata = HashMap::from([
        (
            "event".to_string(),
            Value::String(DATA_DEGRADED_EVENT.to_string()),
        ),
        ("site".to_string(), Value::String(site.to_string())),
        ("detail".to_string(), Value::String(detail.to_string())),
    ]);
    bus.publish_logged(
        BaseEvent {
            id: Id::new(),
            r#type: EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            workflow_id,
            execution_id: Some(execution_id.clone()),
            agent_loop_id: None,
            event_name: None,
            metadata: Some(metadata),
        },
        &format!("execution={} data-degraded site={}", execution_id, site),
    )
    .ok();
}
