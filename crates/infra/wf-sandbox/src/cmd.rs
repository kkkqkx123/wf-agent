//! Reusable policy-driven command hardening gateway.
//!
//! The single place that turns a [`std::process::Command`] into a
//! policy-enforced process: seccomp-bpf (with `AUDIT_ARCH` validation,
//! deny-list or allow-list semantics), rlimits, a cleared environment and
//! Landlock filesystem rules. Both the sandbox execution strategies
//! (e.g. `os-hook`) and the shell/tool executors share this code so every
//! external command runs under the same enforcement instead of each caller
//! re-implementing a weaker variant.
//!
//! All hardening is applied in `pre_exec`, i.e. in the forked child before
//! `exec`, which means the parent process is never constrained and the child
//! starts confined from its first instruction.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use wf_types::script::sandbox::{PathPolicy, SandboxPolicy};

use crate::cmd_landlock;
use crate::cmd_seccomp::build_filter;

/// Options controlling how a command is hardened.
#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    /// When `true` the child starts from a cleared environment plus a minimal
    /// default environment (PATH/HOME/TMPDIR when previously set). Explicit
    /// variables overlaid by the caller afterwards still apply. This prevents
    /// host secrets (tokens, keys, proxy config) from leaking into the
    /// sandboxed process.
    pub clear_env: bool,
    /// When `Some`, Landlock rules enforce the path policy at execution time
    /// on kernels with Landlock support (>= 5.13): each `allowed_read` path
    /// is read-only, each `allowed_write` path is read-write, everything else
    /// is inaccessible. `None`/empty lists disable the enforcement.
    pub path_policy: Option<PathPolicy>,
    /// Working directory of the child, used to absolutize relative Landlock
    /// rule paths. `apply` prefers the command's configured cwd; `apply_tokio`
    /// cannot query it, so set this explicitly when using `apply_tokio` with
    /// a relative path policy.
    pub cwd: Option<PathBuf>,
}

/// Apply policy hardening to a `std::process::Command`. No-op on non-Linux.
///
/// Errors are reported for the caller to decide whether the command should
/// fail closed; a command that cannot be hardened must not silently run
/// unhardened.
pub fn apply(cmd: &mut Command, policy: &SandboxPolicy, opts: &ApplyOptions) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let cwd = cmd
            .get_current_dir()
            .map(Path::to_path_buf)
            .or_else(|| opts.cwd.clone());
        let prepared = prepare(policy, opts, cwd)?;
        if opts.clear_env {
            let overlay = overlay_envs_std(cmd);
            cmd.env_clear();
            apply_minimal_env(
                &mut |k, v| {
                    cmd.env(k, v);
                },
                &overlay,
            );
        }
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(move || {
                pre_exec_setup(
                    &prepared.filters,
                    prepared.memory_limit_mb,
                    prepared.max_file_size,
                    &prepared.path_policy,
                    prepared.cwd.as_deref(),
                )
            });
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (cmd, policy, opts);
        Ok(())
    }
}

/// Apply policy hardening to a `tokio::process::Command` (same semantics as
/// [`apply`], for async call sites using the tokio process API).
///
/// tokio tracks its explicit environment internally: `env_clear` only
/// drops the inherited environment, so variables set before or after this
/// call both survive. Relative Landlock path policies need
/// [`ApplyOptions::cwd`].
pub fn apply_tokio(
    cmd: &mut tokio::process::Command,
    policy: &SandboxPolicy,
    opts: &ApplyOptions,
) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let cwd = opts.cwd.clone();
        let prepared = prepare(policy, opts, cwd)?;
        if opts.clear_env {
            cmd.env_clear();
            apply_minimal_env(
                &mut |k, v| {
                    cmd.env(k, v);
                },
                &[],
            );
        }
        unsafe {
            cmd.pre_exec(move || {
                pre_exec_setup(
                    &prepared.filters,
                    prepared.memory_limit_mb,
                    prepared.max_file_size,
                    &prepared.path_policy,
                    prepared.cwd.as_deref(),
                )
            });
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (cmd, policy, opts);
        Ok(())
    }
}

/// Prepared state shared by the std and tokio entry points.
#[cfg(target_os = "linux")]
struct Prepared {
    filters: Vec<libc::sock_filter>,
    memory_limit_mb: u64,
    max_file_size: u64,
    path_policy: Option<PathPolicy>,
    cwd: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
fn prepare(
    policy: &SandboxPolicy,
    opts: &ApplyOptions,
    cwd: Option<PathBuf>,
) -> io::Result<Prepared> {
    let filters = build_filter(policy)?;
    let memory_limit_mb = policy
        .resource
        .as_ref()
        .and_then(|r| r.memory_limit_mb)
        .unwrap_or(0);
    let max_file_size = policy
        .filesystem
        .as_ref()
        .and_then(|f| f.max_file_size)
        .unwrap_or(0);
    Ok(Prepared {
        filters,
        memory_limit_mb,
        max_file_size,
        path_policy: opts.path_policy.clone(),
        cwd,
    })
}

/// Overlay entries currently set on the command (explicit `Command::env`).
#[cfg(target_os = "linux")]
fn overlay_envs_std(cmd: &Command) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    cmd.get_envs()
        .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v.to_owned())))
        .collect()
}

/// Clear the inherited environment and install a minimal one (default PATH
/// plus HOME/TMPDIR carried over from the parent when set). Explicit
/// variables already set on the command are preserved and re-applied.
fn apply_minimal_env(
    set: &mut dyn FnMut(&std::ffi::OsStr, &std::ffi::OsStr),
    overlay: &[(std::ffi::OsString, std::ffi::OsString)],
) {
    #[cfg(target_os = "linux")]
    {
        let path = std::ffi::OsString::from(
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );
        set(std::ffi::OsStr::new("PATH"), &path);
        if let Ok(home) = std::env::var("HOME") {
            set(std::ffi::OsStr::new("HOME"), std::ffi::OsStr::new(&home));
        }
        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            set(
                std::ffi::OsStr::new("TMPDIR"),
                std::ffi::OsStr::new(&tmpdir),
            );
        }
        for (key, value) in overlay {
            set(key, value);
        }
    }
}

#[cfg(target_os = "linux")]
fn pre_exec_setup(
    filters: &[libc::sock_filter],
    memory_limit_mb: u64,
    max_file_size: u64,
    path_policy: &Option<PathPolicy>,
    cwd: Option<&Path>,
) -> io::Result<()> {
    // Runs in the single-threaded fork child; every raw call here is safe
    // only under that contract (no locks, no allocation, no unwinding).
    unsafe {
        if memory_limit_mb > 0 {
            let bytes = memory_limit_mb * 1024 * 1024;
            let rlim = libc::rlimit {
                rlim_cur: bytes,
                rlim_max: bytes,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &rlim) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if max_file_size > 0 {
            let rlim = libc::rlimit {
                rlim_cur: max_file_size,
                rlim_max: max_file_size,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &rlim) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) < 0 {
            return Err(io::Error::last_os_error());
        }
        if let Some(path_policy) = path_policy {
            if !(path_policy.allowed_read.is_empty() && path_policy.allowed_write.is_empty()) {
                cmd_landlock::apply_landlock(path_policy, cwd)?;
            }
        }
        let prog = libc::sock_fprog {
            len: filters.len() as u16,
            filter: filters.as_ptr() as *mut libc::sock_filter,
        };
        if libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            0i32,
            &prog as *const libc::sock_fprog,
        ) < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
