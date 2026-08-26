//! TCP listener bootstrap shared by the metrics and API routers: bind,
//! serve with graceful shutdown, and expose a `ServerHandle` for the
//! caller to shut the server down. Kept independent of `wf-api` and
//! `wf-metrics`; the caller supplies the composed router.

use std::net::SocketAddr;

use axum::Router;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

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
}

impl ServerHandle {
    /// The address the server is actually bound to (differs from the
    /// requested one when the port was `0`).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Signal graceful shutdown and await the server task, draining
    /// in-flight requests before returning.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// Bind `router` to `addr` and serve it without blocking. Binds immediately
/// and returns a `ServerHandle`; a bind failure surfaces as
/// `Err(ServeError::Bind)` for the caller to decide (e.g. degrade when
/// metrics are an optional service).
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
    })
}
