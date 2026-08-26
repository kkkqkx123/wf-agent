use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::timeout;
use tracing::{error, info};

use crate::error::{RuntimeError, RuntimeResult};

pub struct ShutdownHandle {
    sender: watch::Sender<ShutdownSignal>,
}

pub struct ShutdownWaiter {
    receiver: watch::Receiver<ShutdownSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    Running,
    ShuttingDown,
}

impl ShutdownHandle {
    pub fn trigger(&self) {
        let _ = self.sender.send(ShutdownSignal::ShuttingDown);
    }

    pub fn is_shutting_down(&self) -> bool {
        *self.sender.subscribe().borrow() == ShutdownSignal::ShuttingDown
    }
}

impl ShutdownWaiter {
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow() == ShutdownSignal::ShuttingDown
    }

    pub async fn wait_for_shutdown(&mut self) -> RuntimeResult<()> {
        while self.receiver.changed().await.is_ok() {
            if *self.receiver.borrow() == ShutdownSignal::ShuttingDown {
                return Ok(());
            }
        }
        Err(RuntimeError::Signal(
            "Shutdown channel closed unexpectedly".into(),
        ))
    }
}

pub fn shutdown_channel() -> (ShutdownHandle, ShutdownWaiter) {
    let (sender, receiver) = watch::channel(ShutdownSignal::Running);
    (ShutdownHandle { sender }, ShutdownWaiter { receiver })
}

pub async fn graceful_shutdown<F, Fut>(timeout_duration: Duration, cleanup: F) -> RuntimeResult<()>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = RuntimeResult<()>>,
{
    info!(
        "Starting graceful shutdown with timeout {:?}",
        timeout_duration
    );

    match timeout(timeout_duration, cleanup()).await {
        Ok(Ok(())) => {
            info!("Graceful shutdown completed successfully");
            Ok(())
        }
        Ok(Err(e)) => {
            error!("Graceful shutdown failed with error: {}", e);
            Err(e)
        }
        Err(_) => {
            error!("Graceful shutdown timed out after {:?}", timeout_duration);
            Err(RuntimeError::ShutdownTimeout(
                timeout_duration.as_millis() as u64
            ))
        }
    }
}

type SignalHandler = Pin<Box<dyn Future<Output = RuntimeResult<()>> + Send>>;

pub async fn wait_for_signal() -> RuntimeResult<()> {
    wait_for_sigint().await
}

#[cfg(unix)]
async fn wait_for_sigint() -> RuntimeResult<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|e| RuntimeError::Signal(format!("Failed to register SIGINT handler: {}", e)))?;
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| RuntimeError::Signal(format!("Failed to register SIGTERM handler: {}", e)))?;

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM");
            Ok(())
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_sigint() -> RuntimeResult<()> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| RuntimeError::Signal(format!("Failed to wait for Ctrl+C: {}", e)))?;
    info!("Received Ctrl+C");
    Ok(())
}

pub struct SignalGuard {
    _private: (),
}

pub fn setup_signal_handler<F, Fut>(handler: F) -> RuntimeResult<SignalGuard>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = RuntimeResult<()>> + Send,
{
    let signal_handler: SignalHandler = Box::pin(async move {
        wait_for_signal().await?;
        handler().await
    });

    tokio::spawn(async move {
        if let Err(e) = signal_handler.await {
            error!("Signal handler error: {}", e);
        }
    });

    Ok(SignalGuard { _private: () })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_channel() {
        let (handle, mut waiter) = shutdown_channel();

        assert!(!handle.is_shutting_down());
        assert!(!waiter.is_shutting_down());

        handle.trigger();

        assert!(handle.is_shutting_down());
        waiter.wait_for_shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_graceful_shutdown_success() {
        let result = graceful_shutdown(Duration::from_secs(5), || async { Ok(()) }).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_error() {
        let result = graceful_shutdown(Duration::from_secs(5), || async {
            Err(RuntimeError::Signal("test error".into()))
        })
        .await;

        assert!(matches!(result, Err(RuntimeError::Signal(_))));
    }

    #[tokio::test]
    async fn test_graceful_shutdown_timeout() {
        let result = graceful_shutdown(Duration::from_millis(50), || async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        })
        .await;

        assert!(matches!(result, Err(RuntimeError::ShutdownTimeout(_))));
    }

    #[tokio::test]
    async fn test_shutdown_waiter_is_shutting_down() {
        let (handle, waiter) = shutdown_channel();

        assert!(!waiter.is_shutting_down());

        handle.trigger();

        assert!(waiter.is_shutting_down());
    }
}
