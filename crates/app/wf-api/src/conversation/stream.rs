//! Event subscription for one session.
//!
//! A subscriber gets the events it missed and then the live ones. "Missed" is
//! replayed from the bus's recent ring, which is why a tree turn can finish with
//! nobody listening and still be observed afterwards; the tree itself (not the
//! event log) remains the authority, so a restart loses nothing that matters.

use std::sync::Arc;

use futures::Stream;
use tokio::sync::mpsc;

use wf_types::events::BaseEvent;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Events already published for a session, oldest first.
pub fn replay(ctx: &ApiContext, session_id: &str, since: Option<i64>) -> Vec<BaseEvent> {
    let mut events: Vec<BaseEvent> = ctx
        .event_bus
        .recent_events()
        .into_iter()
        .filter(|event| event.execution_id.as_deref() == Some(session_id))
        .filter(|event| since.is_none_or(|since| event.timestamp > since))
        .collect();
    events.reverse();
    events
}

/// Live events of one session: the replay first, then everything published from
/// now on.
///
/// The replay and the subscription are wired in one spawned task, in that order,
/// so an event published between the two is neither lost nor duplicated.
pub async fn subscribe(
    ctx: Arc<ApiContext>,
    session_id: String,
    since: Option<i64>,
) -> ApiResult<impl Stream<Item = BaseEvent> + Send + 'static> {
    let backlog = replay(&ctx, &session_id, since);
    let bus = ctx.event_bus.clone();
    let (tx, rx) = mpsc::channel(512);
    tokio::spawn(async move {
        for event in backlog {
            if tx.send(event).await.is_err() {
                return;
            }
        }
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.execution_id.as_deref() != Some(session_id.as_str()) {
                        continue;
                    }
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
                // A lagging subscriber lost events; the next snapshot repairs
                // whatever the stream could not deliver.
                Err(_) => continue,
            }
        }
    });
    Ok(futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    }))
}
