//! Event subscription for one session.
//!
//! A subscriber gets the events it missed and then the live ones. The missed
//! events come from the unified history (bus window plus persisted events),
//! which is why a tree turn can finish with nobody listening and still be
//! observed afterwards. The tree itself remains the authority: a restart loses
//! no conversation state even when the event ring has moved on.

use std::collections::HashSet;
use std::sync::Arc;

use futures::Stream;
use tokio::sync::mpsc;

use wf_types::events::BaseEvent;

use super::runtime;
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;

/// Events already published for a session, oldest first.
pub async fn replay(
    ctx: &ApiContext,
    session_id: &str,
    since: Option<i64>,
) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        execution_id: Some(session_id.to_string()),
        limit: Some(500),
        ..EventQueryOptions::default()
    };
    let mut events = crate::infra::events::history(ctx, &options).await?;
    if let Some(since) = since {
        events.retain(|event| event.timestamp > since);
    }
    // History is newest first; a replay must run in publication order.
    events.reverse();
    Ok(events)
}

/// Live events of one session: the replay first, then everything published
/// from now on.
///
/// The live receiver is created before the history query, so an event published
/// between the two cannot fall into the gap. Duplicates are removed by event
/// id: the receiver may also deliver events that history already returned.
pub async fn subscribe(
    ctx: Arc<ApiContext>,
    session_id: String,
    since: Option<i64>,
) -> ApiResult<impl Stream<Item = BaseEvent> + Send + 'static> {
    super::require_chat_session(&ctx, &session_id).await?;
    let runtime = runtime::runtime_for(&session_id);
    let mut subscription = ctx.event_bus.subscribe();
    let backlog = replay(&ctx, &session_id, since).await?;
    runtime.add_subscriber();
    let (tx, rx) = mpsc::channel(512);
    tokio::spawn(async move {
        let mut seen: HashSet<String> = HashSet::new();
        for event in backlog {
            if seen.insert(event.id.clone()) && tx.send(event).await.is_err() {
                return;
            }
        }
        loop {
            if tx.is_closed() {
                return;
            }
            match subscription.recv().await {
                Ok(event) => {
                    if event.execution_id.as_deref() != Some(session_id.as_str()) {
                        continue;
                    }
                    if seen.insert(event.id.clone()) && tx.send(event).await.is_err() {
                        return;
                    }
                }
                // A lagging subscriber lost events; the next snapshot repairs
                // whatever the stream could not deliver.
                Err(_) => continue,
            }
        }
    });
    Ok(futures::stream::unfold(
        (rx, runtime),
        |(mut rx, runtime)| async move {
            let event = rx.recv().await?;
            Some((event, (rx, runtime)))
        },
    ))
}
