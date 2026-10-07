//! Error event subscription over the shared event bus.

use wf_types::events::EventType;

use crate::agent::agent_error_analysis::ExecutionErrorRecord;
use crate::infra::context::ApiContext;

/// Handle returned by [`subscribe_to_errors`]; dropping it
/// stops the subscription.
pub struct ErrorSubscription {
    handle: tokio::task::AbortHandle,
}

impl Drop for ErrorSubscription {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Subscribe to error events of a workflow execution published on the
/// shared event bus. The returned guard aborts the subscription when
/// dropped.
pub fn subscribe_to_errors<F>(
    ctx: &ApiContext,
    execution_id: &str,
    callback: F,
) -> ErrorSubscription
where
    F: Fn(ExecutionErrorRecord) + Send + Sync + 'static,
{
    let bus = ctx.event_bus.clone();
    let mut subscription = bus.subscribe();
    let filter = execution_id.to_string();
    let handle = tokio::spawn(async move {
        while let Ok(event) = subscription.recv().await {
            if event.execution_id.as_deref() != Some(filter.as_str()) {
                continue;
            }
            if event.r#type != EventType::Error {
                continue;
            }
            let message = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("message"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    event
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("error"))
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| "workflow execution error".to_string());
            let node_id = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("node_id"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let error_type = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("error_type"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            callback(ExecutionErrorRecord {
                id: event.id.to_string(),
                execution_id: filter.clone(),
                error: message,
                error_type,
                timestamp: event.timestamp,
                node_id,
                parent_error_id: None,
                error_chain: Vec::new(),
                root_cause_id: String::new(),
                caused_by: None,
                is_recoverable: false,
                recovery_action: None,
            });
        }
    });
    ErrorSubscription {
        handle: handle.abort_handle(),
    }
}
