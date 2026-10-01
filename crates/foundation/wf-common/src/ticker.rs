//! Shared periodic-ticker task: one canonical
//! `interval + MissedTickBehavior::Skip + shutdown` loop so runtime
//! housekeeping tasks (metrics flush, cleanup, GC, reporting) stop
//! re-implementing the same spawn pattern with diverging defaults.

use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Spawn a task that calls `on_tick` every `interval`, skipping missed
/// ticks (a slow tick never backlogs into a burst). The task exits when
/// `shutdown` is cancelled; the returned handle can still be awaited to
/// join it.
///
/// The tick callback is async and must own its data: clone any shared
/// state into the closure before passing it in.
pub fn spawn_ticker<F, Fut>(
    interval: Duration,
    shutdown: CancellationToken,
    on_tick: F,
) -> JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut on_tick = on_tick;
        loop {
            tokio::select! {
                _ = ticker.tick() => on_tick().await,
                _ = shutdown.cancelled() => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn ticker_fires_until_shutdown() {
        let shutdown = CancellationToken::new();
        let count = Arc::new(AtomicU32::new(0));
        let counter = count.clone();
        let handle = spawn_ticker(Duration::from_millis(10), shutdown.clone(), move || {
            let count = counter.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(60)).await;
        shutdown.cancel();
        handle.await.expect("ticker task joins");
        assert!(
            count.load(Ordering::SeqCst) >= 2,
            "ticker should have fired"
        );
    }

    #[tokio::test]
    async fn ticker_exits_immediately_when_shutdown_pre_cancelled() {
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        let handle = spawn_ticker(Duration::from_millis(10), shutdown, || async {});
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("ticker exits without firing")
            .expect("ticker task joins");
    }
}
