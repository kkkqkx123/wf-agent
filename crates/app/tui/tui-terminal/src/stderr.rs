//! Stderr suppression guard: redirects fd 2 to a file while active.

use std::io;

/// Redirects the process stderr (fd 2) into a file while active, so that
/// backend / child-process diagnostics cannot corrupt the rendered screen.
/// `Drop` restores the original stderr.
///
/// Unix does real fd redirection; other platforms get a no-op guard.
#[derive(Debug)]
pub struct TerminalStderrGuard {
    #[cfg(unix)]
    saved_fd: Option<i32>,
    #[cfg(unix)]
    file: Option<std::fs::File>,
    #[cfg(not(unix))]
    _unused: (),
}

impl TerminalStderrGuard {
    /// Redirect fd 2 into `path` (created / truncated).
    #[cfg(unix)]
    pub fn suppress_to(path: &std::path::Path) -> io::Result<Self> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(err);
        }
        Ok(Self {
            saved_fd: Some(saved),
            file: Some(file),
        })
    }

    /// No-op on non-unix platforms.
    #[cfg(not(unix))]
    pub fn suppress_to(_path: &std::path::Path) -> io::Result<Self> {
        Ok(Self { _unused: () })
    }

    /// Restore the original stderr; idempotent. The suppression target file
    /// is kept so [`Self::re_suppress`] can re-apply the redirection.
    #[cfg(unix)]
    pub fn restore(&mut self) -> io::Result<()> {
        if let Some(saved) = self.saved_fd.take() {
            if unsafe { libc::dup2(saved, libc::STDERR_FILENO) } < 0 {
                return Err(io::Error::last_os_error());
            }
            unsafe { libc::close(saved) };
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Re-apply the redirection after a `with_restored` window (no-op when
    /// already suppressing or fully torn down).
    #[cfg(unix)]
    pub fn re_suppress(&mut self) -> io::Result<()> {
        use std::os::unix::io::AsRawFd;
        let Some(file) = self.file.as_ref() else {
            return Ok(());
        };
        if self.saved_fd.is_some() {
            return Ok(());
        }
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(err);
        }
        self.saved_fd = Some(saved);
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn re_suppress(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Whether stderr is currently redirected.
    pub fn is_suppressing(&self) -> bool {
        #[cfg(unix)]
        {
            self.saved_fd.is_some()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

impl Drop for TerminalStderrGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Serializes tests that redirect the process-wide fd 2.
    static STDERR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn suppressed_writes_land_in_the_file_and_restore_releases() {
        let _lock = STDERR_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stderr.log");
        let text = format!("suppressed-{}-line\n", std::process::id());

        {
            let mut guard = TerminalStderrGuard::suppress_to(&path).unwrap();
            assert!(guard.is_suppressing());
            let mut err = io::stderr();
            err.write_all(text.as_bytes()).unwrap();
            err.flush().unwrap();
            guard.restore().unwrap();
            assert!(!guard.is_suppressing());
        }

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(&text), "content was {content:?}");
    }

    #[cfg(unix)]
    mod unix_tests {
        use super::*;
        use crate::terminal::{FakeControl, TerminalGuard, TerminalModes};

        static STDERR_LOCK2: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn with_restored_lifts_and_reapplies_stderr_suppression() {
            let _lock = STDERR_LOCK2.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stderr-cycle.log");
            let mut stderr_guard = TerminalStderrGuard::suppress_to(&path).unwrap();
            assert!(stderr_guard.is_suppressing());

            let mut guard = TerminalGuard::new(FakeControl::default());
            guard.enter(TerminalModes::MINI).unwrap();
            guard.with_restored(Some(&mut stderr_guard), || ()).unwrap();

            // Suppression is active again after the window.
            assert!(stderr_guard.is_suppressing());
            stderr_guard.restore().unwrap();
            assert!(!stderr_guard.is_suppressing());
        }

        #[test]
        fn drop_restores_stderr() {
            let _lock = STDERR_LOCK2.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stderr-drop.log");
            {
                let guard = TerminalStderrGuard::suppress_to(&path).unwrap();
                assert!(guard.is_suppressing());
                drop(guard);
            }
            // The guard is gone; a new one can suppress again (fd
            // bookkeeping stayed balanced).
            let mut guard = TerminalStderrGuard::suppress_to(&path).unwrap();
            assert!(guard.is_suppressing());
            guard.restore().unwrap();
        }
    }
}
