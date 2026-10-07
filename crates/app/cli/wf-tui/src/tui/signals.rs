//! Terminal signal plumbing for the TUI shell: Ctrl-Z suspend request
//! recording and SIGUSR2 theme hot-reload delivery.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use libc;
use tokio::sync::mpsc;

/// Set by the SIGTSTP handler when the user suspends the app (Ctrl-Z); the
/// event loop observes it and runs the suspend / resume cycle. Only a flag
/// store happens inside the handler, which is async-signal-safe.
pub(super) static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// SIGTSTP handler: record the suspension request. The actual terminal
/// restore / `SIGSTOP` sequence runs in the event loop (not here) so it can
/// use normal Rust calls.
pub(super) extern "C" fn sigtstp_handler(_sig: libc::c_int) {
    SUSPEND_PENDING.store(true, Ordering::SeqCst);
}

/// Channel delivering one `()` per SIGUSR2 theme hot-reload request.
/// Non-unix platforms get an immediately-closed channel. Owned by the
/// application shell so `tui-style` never depends on an async runtime.
#[cfg(unix)]
pub(super) async fn theme_reload_signals() -> io::Result<mpsc::Receiver<()>> {
    use tokio::signal::unix::{signal, SignalKind};

    let (tx, rx) = mpsc::channel(8);
    let mut stream = signal(SignalKind::user_defined2())?;
    tokio::spawn(async move {
        while stream.recv().await.is_some() {
            if tx.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(rx)
}

#[cfg(not(unix))]
pub(super) async fn theme_reload_signals() -> io::Result<mpsc::Receiver<()>> {
    let (_tx, rx) = mpsc::channel::<()>(8);
    Ok(rx)
}
