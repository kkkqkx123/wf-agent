//! TCP listener bootstrap shared by the metrics and API routers: bind,
//! serve with graceful shutdown, and expose a `ServerHandle` for the
//! caller to shut the server down. Kept independent of `wf-api` and
//! `wf-metrics`; the caller supplies the composed router.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Default ceiling for how long a graceful drain waits for in-flight HTTP
/// requests before the listener is force-closed. SSE long connections never
/// end on their own, so without a bound `axum`'s graceful shutdown can hang
/// until an external supervisor kills the process.
pub const DEFAULT_SHUTDOWN_DRAIN_SECS: u64 = 10;

#[cfg(feature = "backends")]
pub(crate) type BackendHandle = Option<std::sync::Arc<crate::backends::BackendManager>>;
#[cfg(not(feature = "backends"))]
pub(crate) type BackendHandle = Option<()>;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("bind failed: {0}")]
    Bind(String),
    #[error("server error: {0}")]
    Server(String),
}

/// Handle to a running HTTP server: the actually bound address and
/// a graceful shutdown signal. Dropping the handle without `shutdown` lets
/// the server task keep running until the runtime stops.
pub struct ServerHandle {
    pub(crate) addr: SocketAddr,
    pub(crate) shutdown: oneshot::Sender<()>,
    pub(crate) task: JoinHandle<()>,
    pub(crate) backends: BackendHandle,
    /// Maximum time to wait for in-flight requests to finish after the
    /// graceful-shutdown signal. Exceeding it aborts the serve task, which
    /// force-closes remaining (e.g. SSE) connections.
    pub(crate) drain: Duration,
    /// Optional background detached-run reaper and its cancellation token.
    /// Owned here so shutdown stops it before backends/tunnels are torn down.
    pub(crate) reaper: Option<(JoinHandle<()>, tokio_util::sync::CancellationToken)>,
}

impl ServerHandle {
    /// The address the server is actually bound to (differs from the
    /// requested one when the port was `0`).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Override the graceful-drain ceiling. `None` (the default used by
    /// metrics and test servers) keeps [`DEFAULT_SHUTDOWN_DRAIN_SECS`].
    pub fn with_drain(mut self, drain: Duration) -> Self {
        self.drain = drain;
        self
    }

    /// Attach a background detached-run reaper to this server's lifecycle.
    /// The reaper is cancelled and awaited during `shutdown`, before tunnels
    /// are torn down, so no sweep races with backend teardown.
    pub(crate) fn with_reaper(
        mut self,
        handle: JoinHandle<()>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Self {
        self.reaper = Some((handle, cancel));
        self
    }

    /// Signal graceful shutdown, drain in-flight requests under a hard time
    /// budget, then tear down backend tunnels.
    ///
    /// Ordering matters. The shutdown signal is sent first so the listener
    /// stops accepting new work immediately; the serve task is then awaited
    /// with a timeout because SSE long connections never close on their own
    /// and an unbounded drain would hang until an external killer intervenes.
    /// Tunnels are stopped only after the drain: they are process-level and
    /// shared by every session, so tearing them down while requests are still
    /// in flight would cut off in-flight backup fetches (see the shutdown
    /// design doc). Hence no `ssh -N` outlives the service, but none is killed
    /// prematurely either.
    pub async fn shutdown(self) {
        crate::api::system::server::mark_server_stopping();
        // A request may already have passed the stopping guard; the manager
        // switch closes that window before the HTTP drain starts.
        #[cfg(feature = "backends")]
        if let Some(backends) = &self.backends {
            backends.stop_accepting_deploy_jobs();
        }
        let _ = self.shutdown.send(());

        let mut task = self.task;
        match tokio::time::timeout(self.drain, &mut task).await {
            // The timeout future already polled `&mut task` to completion and
            // consumed its output. Awaiting it again here would panic with
            // "JoinHandle polled after completion", so only inspect the result.
            Ok(result) => {
                if let Err(err) = result {
                    tracing::warn!(target: "wf_server", error = %err, "HTTP serve task exited with an error");
                }
            }
            // The task is still pending: abort it, then reap exactly once.
            Err(_) => {
                tracing::warn!(
                    target: "wf_server",
                    drain_secs = self.drain.as_secs(),
                    "graceful drain exceeded budget; force-closing remaining connections"
                );
                task.abort();
                let _ = task.await;
            }
        }

        // Converge in-flight deployment jobs while the HTTP surface is still
        // alive and tunnels are up: a cancelled step can settle to a clean
        // `interrupted` terminal. This must precede tunnel teardown, after
        // which SSH steps would fail and force a `failed` terminal instead.
        #[cfg(feature = "backends")]
        if let Some(backends) = &self.backends {
            backends.cancel_deploy_jobs().await;
        }

        // Stop the detached-run reaper first: it may still be settling an
        // idle run, and we want that done before tunnels disappear.
        if let Some((reaper, cancel)) = self.reaper {
            cancel.cancel();
            let _ = reaper.await;
        }

        // Converge PTY sessions after the HTTP drain but before tunnels die:
        // no new session can arrive (stopping flag set), and any in-flight
        // attach has already been drained. Kills every child, persistent or
        // not, so no shell outlives the service.
        #[cfg(feature = "pty")]
        {
            let killed = wf_api::pty::global_manager().shutdown_all();
            if killed > 0 {
                tracing::info!(target: "wf_server", killed, "stopped PTY sessions on shutdown");
            }
        }

        #[cfg(feature = "backends")]
        if let Some(backends) = self.backends {
            backends.shutdown().await;
        }
        #[cfg(not(feature = "backends"))]
        let _ = self.backends;
    }
}

/// Bind `router` to `addr` and serve it without blocking. Binds immediately
/// and returns a `ServerHandle`; a bind failure surfaces as
/// `Err(ServeError::Bind)` for the caller to decide (e.g. degrade when
/// metrics are an optional service).
#[cfg(any(feature = "metrics", test))]
pub(crate) async fn serve_with_router(
    router: Router,
    addr: SocketAddr,
) -> Result<ServerHandle, ServeError> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let bound_addr = listener
        .local_addr()
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
        {
            tracing::error!(target: "wf_server", error = %err, "HTTP server failed");
        }
    });
    Ok(ServerHandle {
        addr: bound_addr,
        shutdown: shutdown_tx,
        task,
        backends: None,
        drain: Duration::from_secs(DEFAULT_SHUTDOWN_DRAIN_SECS),
        reaper: None,
    })
}

/// Same as [`serve_with_router`] with an explicit backend manager for
/// graceful tunnel and loop shutdown. `None` keeps the previous
/// behavior for callers without the backend domain.
pub(crate) async fn serve_with_router_and_backends(
    router: Router,
    addr: SocketAddr,
    backends: BackendHandle,
) -> Result<ServerHandle, ServeError> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let bound_addr = listener
        .local_addr()
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
        {
            tracing::error!(target: "wf_server", error = %err, "HTTP server failed");
        }
    });
    Ok(ServerHandle {
        addr: bound_addr,
        shutdown: shutdown_tx,
        task,
        backends,
        drain: Duration::from_secs(DEFAULT_SHUTDOWN_DRAIN_SECS),
        reaper: None,
    })
}

#[cfg(all(test, feature = "backends"))]
mod tests {
    use super::*;
    use axum::response::sse::{Event, Sse};
    use axum::routing::get;
    use std::convert::Infallible;
    use std::time::Instant;

    /// A server whose only connection is a streaming SSE response never
    /// drains on its own. Shutdown must still return within the configured
    /// budget, force-closing the connection after the timeout.
    #[tokio::test]
    async fn shutdown_is_bounded_by_drain_budget() {
        async fn never_ends() -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
            let stream = futures::stream::pending::<Result<Event, Infallible>>();
            Sse::new(stream)
        }
        let router = Router::new().route("/sse", get(never_ends));
        // Bind on an ephemeral port so parallel tests never collide.
        let handle = serve_with_router(router, "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap()
            .with_drain(Duration::from_millis(150));
        let addr = handle.addr();

        // Open a streaming TCP connection that stays open (SSE handshake),
        // so the serve task has a live connection to wait on during drain.
        // Hold `_client` until after shutdown: dropping it early would let
        // the drain finish before the timeout and defeat the assertion.
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        client
            .write_all(b"GET /sse HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        // Give the listener a moment to accept the connection before we
        // signal shutdown.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let started = Instant::now();
        handle.shutdown().await;
        let elapsed = started.elapsed();
        drop(client);
        assert!(
            elapsed < Duration::from_secs(2),
            "shutdown took {elapsed:?}, expected to be bounded near 150ms"
        );
    }
}
