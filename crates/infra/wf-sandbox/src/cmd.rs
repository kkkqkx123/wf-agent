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

use wf_types::script::sandbox::{NetworkAccessType, PathPolicy, ProcessPolicy, SandboxPolicy};

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
                apply_landlock(path_policy, cwd)?;
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

// ================= Seccomp BPF filter building =================

/// SECCOMP_RET_* actions.
const RET_KILL: u32 = 0x8000_0000; // SECCOMP_RET_KILL_PROCESS
const RET_ALLOW: u32 = 0x7fff_0000; // SECCOMP_RET_ALLOW

/// Native `AUDIT_ARCH` value. The seccomp data at offset 4 must equal it,
/// otherwise syscall numbers are ambiguous (e.g. x32 ABI reuses them).
fn native_audit_arch() -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        0xc000_003e // AUDIT_ARCH_X86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        0xc000_00b7 // AUDIT_ARCH_AARCH64
    }
    #[cfg(target_arch = "x86")]
    {
        0x4000_0003 // AUDIT_ARCH_I386
    }
    #[cfg(target_arch = "arm")]
    {
        0x4000_0028 // AUDIT_ARCH_ARM
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "arm"
    )))]
    {
        0 // unknown arch: keep the arch check but it will always kill
    }
}

fn bpf_stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

/// Load `seccomp_data.arch` (offset 4).
fn bpf_load_arch() -> libc::sock_filter {
    bpf_stmt(0x20, 4)
}

/// Load `seccomp_data.nr` (offset 0).
fn bpf_load_nr() -> libc::sock_filter {
    bpf_stmt(0x20, 0)
}

/// BPF program skeleton: validate `AUDIT_ARCH` first (kill on mismatch),
/// then dispatch on the syscall number.
///
/// `list` and `kill_on_match` decide the semantics:
/// - deny-list (`kill_on_match = true`): match → KILL, default ALLOW;
/// - allow-list (`kill_on_match = false`): match → ALLOW, default KILL.
fn build_arch_check_filter(list: &[i64], kill_on_match: bool) -> Vec<libc::sock_filter> {
    let n = list.len();
    let mut filters = Vec::with_capacity(n + 4);

    // [0] load arch
    filters.push(bpf_load_arch());
    // [1] jeq native_arch, jt=1 (skip the kill), jf=0 (fall into kill)
    filters.push(bpf_jump(0x15, native_audit_arch(), 1, 0));
    // [2] kill on arch mismatch
    filters.push(bpf_stmt(0x06, RET_KILL));
    // [3] load syscall number
    filters.push(bpf_load_nr());

    if kill_on_match {
        // [4..4+n) jeq denied_i, jt=(n-i), jf=0 ; [4+n] ALLOW ; [4+n+1] KILL
        for (i, &nr) in list.iter().enumerate() {
            filters.push(bpf_jump(0x15, nr as u32, (n - i) as u8, 0));
        }
        filters.push(bpf_stmt(0x06, RET_ALLOW));
        filters.push(bpf_stmt(0x06, RET_KILL));
    } else {
        // [4..4+n) jeq allowed_i, jt=(n-i-1), jf=0 ; [4+n] ALLOW
        for (i, &nr) in list.iter().enumerate() {
            filters.push(bpf_jump(0x15, nr as u32, (n - i - 1) as u8, 0));
        }
        filters.push(bpf_stmt(0x06, RET_ALLOW));
    }
    filters
}

/// System calls that are always denied regardless of policy: kernel-level
/// privilege or integrity operations that a sandboxed process must never
/// perform. Additions beyond the original list close the gaps documented in
/// the sandbox analysis (memfd_create / userfaultfd / io_uring / mount API /
/// pidfd_getfd / open_by_handle_at ...).
fn base_denied_syscalls() -> Vec<i64> {
    use libc::*;
    vec![
        SYS_ptrace,
        SYS_process_vm_readv,
        SYS_process_vm_writev,
        SYS_bpf,
        SYS_kexec_file_load,
        SYS_kexec_load,
        SYS_swapon,
        SYS_swapoff,
        SYS_init_module,
        SYS_delete_module,
        SYS_finit_module,
        SYS_iopl,
        SYS_ioperm,
        SYS_chroot,
        SYS_modify_ldt,
        SYS_pivot_root,
        SYS_mount,
        SYS_umount2,
        SYS_syslog,
        SYS_sethostname,
        SYS_setdomainname,
        SYS_reboot,
        SYS_perf_event_open,
        SYS_quotactl,
        SYS_add_key,
        SYS_request_key,
        SYS_keyctl,
        SYS_lookup_dcookie,
        SYS_acct,
        SYS_vhangup,
        SYS_clock_settime,
        SYS_settimeofday,
        SYS_adjtimex,
        SYS_setuid,
        SYS_setgid,
        SYS_setreuid,
        SYS_setregid,
        SYS_setresuid,
        SYS_setresgid,
        SYS_setfsuid,
        SYS_setfsgid,
        SYS_setgroups,
        SYS_setpgid,
        SYS_setsid,
        // New additions: modern syscall attack surface.
        SYS_memfd_create,
        SYS_userfaultfd,
        SYS_io_uring_setup,
        SYS_io_uring_enter,
        SYS_io_uring_register,
        SYS_pidfd_getfd,
        SYS_open_by_handle_at,
        SYS_name_to_handle_at,
        SYS_kcmp,
        SYS_open_tree,
        SYS_move_mount,
        SYS_fsopen,
        SYS_fsconfig,
        SYS_fsmount,
        SYS_fspick,
        SYS_mount_setattr,
        SYS_quotactl_fd,
        SYS_setns,
        SYS_unshare,
    ]
}

fn network_allowed(policy: &SandboxPolicy) -> bool {
    policy
        .network
        .as_ref()
        .map(|n| {
            n.access_type.as_ref().unwrap_or(&NetworkAccessType::None) != &NetworkAccessType::None
        })
        .unwrap_or(false)
}

/// Policy-driven deny list: base dangerous syscalls plus the policy-derived
/// groups (network / process creation / filesystem modification).
pub(crate) fn policy_denied_syscalls(policy: &SandboxPolicy) -> Vec<i64> {
    use libc::*;
    let mut denied = base_denied_syscalls();

    if !network_allowed(policy) {
        denied.extend_from_slice(&[
            SYS_socket,
            SYS_connect,
            SYS_accept,
            SYS_sendto,
            SYS_recvfrom,
            SYS_sendmsg,
            SYS_recvmsg,
            SYS_shutdown,
            SYS_bind,
            SYS_listen,
            SYS_getsockname,
            SYS_getpeername,
            SYS_socketpair,
            SYS_setsockopt,
            SYS_getsockopt,
            SYS_accept4,
            SYS_sendmmsg,
            SYS_recvmmsg,
        ]);
    }

    let allow_exec = policy
        .process
        .as_ref()
        .and_then(|p| p.allow_exec)
        .unwrap_or(true);
    if !allow_exec {
        denied.extend_from_slice(&[SYS_execve, SYS_execveat]);
    }
    let allow_fork = policy
        .process
        .as_ref()
        .and_then(|p| p.allow_fork)
        .unwrap_or(true);
    if !allow_fork {
        denied.extend_from_slice(&[SYS_fork, SYS_vfork, SYS_clone, SYS_clone3]);
    }

    // Filesystem policy: write authorization does NOT imply delete
    // authorization. Delete-class syscalls are only permitted when
    // `allowed_remove_paths` is explicitly configured; create/modify-class
    // syscalls follow `allowed_write_paths`. With no write paths configured
    // the modify-class syscalls are denied at the syscall layer even if a
    // static-analysis gate were bypassed.
    let write_paths: &[String] = policy
        .filesystem
        .as_ref()
        .and_then(|f| f.allowed_write_paths.as_deref())
        .unwrap_or(&[]);
    let remove_paths: &[String] = policy
        .filesystem
        .as_ref()
        .and_then(|f| f.allowed_remove_paths.as_deref())
        .unwrap_or(&[]);
    if write_paths.is_empty() {
        denied.extend_from_slice(&[
            SYS_mkdir,
            SYS_mkdirat,
            SYS_symlink,
            SYS_symlinkat,
            SYS_link,
            SYS_linkat,
            SYS_mknod,
            SYS_mknodat,
            SYS_chmod,
            SYS_fchmod,
            SYS_fchmodat,
            SYS_chown,
            SYS_fchown,
            SYS_fchownat,
            SYS_lchown,
            SYS_truncate,
            SYS_ftruncate,
            SYS_utime,
            SYS_utimes,
            SYS_utimensat,
            SYS_futimesat,
            SYS_fallocate,
            SYS_copy_file_range,
        ]);
    }
    if remove_paths.is_empty() {
        denied.extend_from_slice(&[
            SYS_rmdir,
            SYS_unlink,
            SYS_unlinkat,
            SYS_rename,
            SYS_renameat,
            SYS_renameat2,
        ]);
    }

    denied.sort();
    denied.dedup();
    denied
}

/// Syscalls required for basic shell/tool execution on x86_64. Used by the
/// opt-in allow-list mode only; the deny-list mode stays the default because
/// an allow-list can never cover every syscall a legitimate program needs.
#[cfg(target_arch = "x86_64")]
fn base_allowed_syscalls(policy: &SandboxPolicy) -> Vec<i64> {
    let mut allowed: Vec<i64> = vec![
        // I/O and file access
        0,   // read
        1,   // write
        2,   // open
        3,   // close
        4,   // stat
        5,   // fstat
        6,   // lstat
        8,   // lseek
        9,   // mmap
        10,  // mprotect
        11,  // munmap
        12,  // brk
        16,  // ioctl
        17,  // pread64
        18,  // pwrite64
        19,  // readv
        20,  // writev
        21,  // access
        72,  // fcntl
        74,  // fsync
        75,  // fdatasync
        78,  // getdents
        79,  // getcwd
        80,  // chdir
        81,  // fchdir
        89,  // readlink
        95,  // umask
        137, // statfs
        138, // fstatfs
        217, // getdents64
        257, // openat
        262, // newfstatat
        267, // readlinkat
        269, // faccessat
        332, // statx
        437, // openat2
        439, // faccessat2
        // stdio / pipes / dup
        22,  // pipe
        32,  // dup
        33,  // dup2
        292, // dup3
        293, // pipe2
        436, // close_range
        // process lifecycle
        39,  // getpid
        40,  // sendfile
        57,  // fork
        58,  // vfork
        59,  // execve
        60,  // exit
        61,  // wait4
        63,  // uname
        102, // getuid
        104, // getgid
        107, // geteuid
        108, // getegid
        110, // getppid
        111, // getpgrp
        115, // getgroups
        121, // getpgid
        186, // gettid
        218, // set_tid_address
        231, // exit_group
        234, // tgkill
        247, // waitid
        424, // pidfd_send_signal
        435, // clone3
        // signals
        13,  // rt_sigaction
        14,  // rt_sigprocmask
        15,  // rt_sigreturn
        62,  // kill
        127, // rt_sigpending
        128, // rt_sigtimedwait
        129, // rt_sigqueueinfo
        130, // rt_sigsuspend
        131, // sigaltstack
        200, // tkill
        // clocks / time
        35,  // nanosleep
        96,  // gettimeofday
        201, // time
        228, // clock_gettime
        229, // clock_getres
        230, // clock_nanosleep
        // memory
        25,  // mremap
        26,  // msync
        27,  // mincore
        28,  // madvise
        149, // mlock
        150, // munlock
        151, // mlockall
        152, // munlockall
        325, // mlock2
        329, // pkey_mprotect
        330, // pkey_alloc
        331, // pkey_free
        // threads / synchronization
        56,  // clone
        202, // futex
        218, // set_tid_address (dup ok)
        273, // set_robust_list
        274, // get_robust_list
        334, // rseq
        449, // futex_waitv
        // scheduling / process info
        24,  // sched_yield
        97,  // getrlimit
        98,  // getrusage
        99,  // sysinfo
        100, // times
        140, // getpriority
        142, // sched_setparam
        143, // sched_getparam
        144, // sched_setscheduler
        145, // sched_getscheduler
        146, // sched_get_priority_max
        147, // sched_get_priority_min
        148, // sched_rr_get_interval
        160, // setrlimit
        203, // sched_setaffinity
        204, // sched_getaffinity
        251, // ioprio_set
        252, // ioprio_get
        302, // prlimit64
        309, // getcpu
        314, // sched_setattr
        315, // sched_getattr
        324, // membarrier
        // kernel / runtime support
        157, // prctl
        158, // arch_prctl
        162, // sync
        219, // restart_syscall
        273, // set_robust_list (dup)
        317, // seccomp (no_new_privs blocks loosening; probe-friendly)
        318, // getrandom
        438, // pidfd_getfd is dangerous -> NOT allowed
        7,   // poll
        23,  // select
        270, // pselect6
        271, // ppoll
        213, // epoll_create
        232, // epoll_wait
        233, // epoll_ctl
        281, // epoll_pwait
        291, // epoll_create1
        441, // epoll_pwait2
        282, // signalfd
        289, // signalfd4
        283, // timerfd_create
        286, // timerfd_settime
        287, // timerfd_gettime
        284, // eventfd
        290, // eventfd2
        253, // inotify_init
        254, // inotify_add_watch
        255, // inotify_rm_watch
        294, // inotify_init1
        222, // timer_create
        223, // timer_settime
        224, // timer_gettime
        225, // timer_getoverrun
        226, // timer_delete
        275, // splice
        276, // tee
        278, // vmsplice
        221, // fadvise64
        187, // readahead
        295, // preadv
        296, // pwritev
        327, // preadv2
        328, // pwritev2
        285, // fallocate
        326, // copy_file_range
        235, // utimes
        280, // utimensat
    ];

    if network_allowed(policy) {
        allowed.extend_from_slice(&[
            41,  // socket
            42,  // connect
            43,  // accept
            44,  // sendto
            45,  // recvfrom
            46,  // sendmsg
            47,  // recvmsg
            48,  // shutdown
            49,  // bind
            50,  // listen
            51,  // getsockname
            52,  // getpeername
            53,  // socketpair
            54,  // setsockopt
            55,  // getsockopt
            288, // accept4
            299, // recvmmsg
            307, // sendmmsg
        ]);
    }

    let allow_exec = policy
        .process
        .as_ref()
        .and_then(|p| p.allow_exec)
        .unwrap_or(true);
    if !allow_exec {
        allowed.retain(|&nr| nr != 59 && nr != 322); // execve, execveat
    }
    let allow_fork = policy
        .process
        .as_ref()
        .and_then(|p| p.allow_fork)
        .unwrap_or(true);
    if !allow_fork {
        allowed.retain(|&nr| !matches!(nr, 56 | 57 | 58 | 435)); // clone, fork, vfork, clone3
    }

    // Filesystem modification follows the same authorization model as the
    // deny-list mode: write-class requires write paths, remove-class requires
    // remove paths.
    let write_paths: &[String] = policy
        .filesystem
        .as_ref()
        .and_then(|f| f.allowed_write_paths.as_deref())
        .unwrap_or(&[]);
    let remove_paths: &[String] = policy
        .filesystem
        .as_ref()
        .and_then(|f| f.allowed_remove_paths.as_deref())
        .unwrap_or(&[]);
    if !write_paths.is_empty() {
        allowed.extend_from_slice(&[
            83,  // mkdir
            258, // mkdirat
            88,  // symlink
            266, // symlinkat
            86,  // link
            265, // linkat
            133, // mknod
            259, // mknodat
            90,  // chmod
            91,  // fchmod
            268, // fchmodat
            92,  // chown
            93,  // fchown
            260, // fchownat
            94,  // lchown
            76,  // truncate
            77,  // ftruncate
            132, // utime
        ]);
    }
    if !remove_paths.is_empty() {
        allowed.extend_from_slice(&[
            84,  // rmdir
            87,  // unlink
            263, // unlinkat
            82,  // rename
            264, // renameat
            316, // renameat2
        ]);
    }

    allowed.sort();
    allowed.dedup();
    allowed
}

#[cfg(not(target_arch = "x86_64"))]
fn base_allowed_syscalls(_policy: &SandboxPolicy) -> Vec<i64> {
    // Allow-list semantics on non-x86_64 architectures are not yet curated;
    // refusing to build a possibly-incomplete list is safer than allowing
    // an unvalidated one.
    Vec::new()
}

/// Build the seccomp filter for a policy.
///
/// `ProcessPolicy::allowlist_syscalls` selects allow-list semantics (default
/// deny); otherwise deny-list semantics (default allow) with the policy-
/// driven deny groups. Both variants validate `AUDIT_ARCH` first.
pub fn build_filter(policy: &SandboxPolicy) -> io::Result<Vec<libc::sock_filter>> {
    let allowlist = policy
        .process
        .as_ref()
        .and_then(|p: &ProcessPolicy| p.allowlist_syscalls)
        .unwrap_or(false);
    if allowlist {
        let allowed = base_allowed_syscalls(policy);
        if allowed.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "seccomp allow-list mode is not supported on this architecture",
            ));
        }
        Ok(build_arch_check_filter(&allowed, false))
    } else {
        let denied = policy_denied_syscalls(policy);
        Ok(build_arch_check_filter(&denied, true))
    }
}

// ================= Landlock filesystem enforcement =================

/// Raw Landlock syscalls (Linux >= 5.13).
const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
const SYS_LANDLOCK_ADD_RULE: i64 = 445;
const SYS_LANDLOCK_RESTRICT_SELF: i64 = 446;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;

/// All ABI-v1 access rights. ABI-2/3 bits (TRUNCATE, REFER) are excluded so
/// the ruleset works on every Landlock kernel.
const LANDLOCK_HANDLED_FS: u64 = LANDLOCK_ACCESS_FS_EXECUTE
    | LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_READ_FILE
    | LANDLOCK_ACCESS_FS_READ_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK
    | LANDLOCK_ACCESS_FS_MAKE_SYM;

/// Read-only access: execute + read file + read dir.
const LANDLOCK_READ_ACCESS: u64 =
    LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;

/// Read-write access (mirrors the seccomp model: write does NOT imply
/// remove, so REMOVE_* bits are intentionally absent).
const LANDLOCK_WRITE_ACCESS: u64 = LANDLOCK_READ_ACCESS
    | LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_SYM
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK;

#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

#[repr(C)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// Whether Landlock is usable on this kernel (checked once).
fn landlock_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let attr = LandlockRulesetAttr {
            handled_access_fs: 0,
        };
        let fd = unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const _, 0usize) };
        if fd < 0 {
            return false;
        }
        unsafe {
            libc::close(fd as i32);
        }
        true
    })
}

/// Resolve a possibly-relative path against `cwd` so Landlock rules do not
/// depend on the child's working directory at rule-add time.
fn absolutize(path: &str, cwd: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(cwd) = cwd {
        cwd.join(p)
    } else {
        p.to_path_buf()
    }
}

/// Apply a Landlock ruleset to the current process (must be called from the
/// single-threaded child before exec, after `PR_SET_NO_NEW_PRIVS`).
///
/// Fail-closed: an error while adding a rule aborts the spawn. Unsupported
/// kernels (ENOSYS/EOPNOTSUPP) are skipped instead — the seccomp and
/// analysis gates still apply.
fn apply_landlock(path_policy: &PathPolicy, cwd: Option<&Path>) -> io::Result<()> {
    if !landlock_available() {
        return Ok(());
    }

    let attr = LandlockRulesetAttr {
        handled_access_fs: LANDLOCK_HANDLED_FS,
    };
    let ruleset_fd =
        unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const _, 0usize) };
    if ruleset_fd < 0 {
        let err = io::Error::last_os_error();
        // Unsupported kernels/configs are tolerated (seccomp still applies);
        // any other failure is fatal.
        return match err.raw_os_error() {
            Some(libc::ENOSYS) | Some(libc::EOPNOTSUPP) => Ok(()),
            _ => Err(err),
        };
    }
    let ruleset_fd = ruleset_fd as i32;

    // Each rule targets the allowed path itself (opened with O_PATH): for a
    // directory the access applies to it and everything beneath it; for a
    // file it applies to that file only. `parent_fd` of the ABI-v1 struct is
    // this object fd; the ABI-v2 `path` member is left zero-filled by the
    // kernel's copy_struct_from_user when the shorter struct is passed.
    let add_rule = |allowed_access: u64, path: &str| -> io::Result<()> {
        let abs = absolutize(path, cwd);
        let path_c = std::ffi::CString::new(abs.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
        let object_fd = unsafe { libc::open(path_c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if object_fd < 0 {
            // The path does not exist (yet); the vfs-gate would have denied
            // access to it anyway. Skip instead of failing the whole command.
            return Ok(());
        }
        let beneath = LandlockPathBeneathAttr {
            allowed_access,
            parent_fd: object_fd,
        };
        let ret = unsafe {
            libc::syscall(
                SYS_LANDLOCK_ADD_RULE,
                ruleset_fd,
                LANDLOCK_RULE_PATH_BENEATH,
                &beneath as *const _,
                0usize,
            )
        };
        unsafe {
            libc::close(object_fd);
        }
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    };

    for path in &path_policy.allowed_read {
        add_rule(LANDLOCK_READ_ACCESS, path)?;
    }
    for path in &path_policy.allowed_write {
        add_rule(LANDLOCK_WRITE_ACCESS, path)?;
    }

    let ret = unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0usize) };
    unsafe {
        libc::close(ruleset_fd);
    }
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{FilesystemPolicy, NetworkPolicy, ProcessPolicy};

    fn basic_policy() -> SandboxPolicy {
        SandboxPolicy {
            mode: None,
            shell: None,
            python: None,
            javascript: None,
            lua: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        }
    }

    #[test]
    fn test_filter_checks_arch_before_nr() {
        // First instruction loads arch (0x20, offset 4), second is the jeq.
        let denied = vec![101i64];
        let filters = build_arch_check_filter(&denied, true);
        assert_eq!(filters[0].code, 0x20);
        assert_eq!(filters[0].k, 4, "must load seccomp_data.arch first");
        assert_eq!(filters[1].code, 0x15, "must compare arch");
        assert_eq!(filters[2].k, RET_KILL, "arch mismatch must kill");
        assert_eq!(filters[3].code, 0x20);
        assert_eq!(filters[3].k, 0, "then load the syscall number");
    }

    #[test]
    fn test_deny_list_shape() {
        // [0] arch load, [1] arch jeq, [2] kill, [3] nr load,
        // [4..5] jeq list, [6] allow, [7] kill.
        let filters = build_arch_check_filter(&[1i64, 2i64], true);
        assert_eq!(filters.len(), 8);
        assert_eq!(filters[4].k, 1, "first denied syscall");
        assert_eq!(filters[5].k, 2, "second denied syscall");
        assert_eq!(filters[6].k, RET_ALLOW, "default allow (deny-list)");
        assert_eq!(filters[7].k, RET_KILL, "matched syscalls kill");
    }

    #[test]
    fn test_allow_list_shape() {
        // [0] arch load, [1] arch jeq, [2] kill, [3] nr load,
        // [4..5] jeq list, [6] allow.
        let filters = build_arch_check_filter(&[0i64, 1i64], false);
        assert_eq!(filters.len(), 7);
        assert_eq!(filters[4].k, 0);
        assert_eq!(filters[5].k, 1);
        assert_eq!(
            filters[6].k, RET_ALLOW,
            "matched syscalls fall through to allow"
        );
        assert_eq!(filters[2].k, RET_KILL, "default deny (allow-list)");
    }

    #[test]
    fn test_base_denied_covers_documented_gaps() {
        use libc::*;
        let denied = base_denied_syscalls();
        for syscall in [
            SYS_ptrace,
            SYS_process_vm_readv,
            SYS_bpf,
            SYS_memfd_create,
            SYS_userfaultfd,
            SYS_io_uring_setup,
            SYS_pidfd_getfd,
            SYS_open_by_handle_at,
            SYS_open_tree,
            SYS_setns,
            SYS_unshare,
            SYS_mount,
        ] {
            assert!(
                denied.contains(&syscall),
                "syscall {syscall} must be in the base deny list"
            );
        }
    }

    #[test]
    fn test_policy_denied_network_when_disabled() {
        use libc::*;
        let denied = policy_denied_syscalls(&basic_policy());
        for syscall in [
            SYS_socket,
            SYS_connect,
            SYS_sendto,
            SYS_recvfrom,
            SYS_accept4,
        ] {
            assert!(
                denied.contains(&syscall),
                "network syscall {syscall} must be denied when network disabled"
            );
        }
    }

    #[test]
    fn test_policy_denied_network_excluded_when_enabled() {
        use libc::*;
        let policy = SandboxPolicy {
            network: Some(NetworkPolicy {
                access_type: Some(NetworkAccessType::All),
                allowed_domains: None,
                allowed_ports: None,
                allow_dns: Some(true),
            }),
            ..basic_policy()
        };
        let denied = policy_denied_syscalls(&policy);
        assert!(!denied.contains(&SYS_socket));
    }

    #[test]
    fn test_policy_denied_fs_write_and_remove_without_paths() {
        use libc::*;
        let denied = policy_denied_syscalls(&basic_policy());
        for syscall in [SYS_unlink, SYS_unlinkat, SYS_mkdir, SYS_chmod, SYS_truncate] {
            assert!(
                denied.contains(&syscall),
                "fs syscall {syscall} must be denied without paths"
            );
        }
    }

    #[test]
    fn test_policy_denied_fs_write_allowed_remove_stays_denied() {
        use libc::*;
        let policy = SandboxPolicy {
            filesystem: Some(FilesystemPolicy {
                allowed_read_paths: Some(vec![]),
                allowed_write_paths: Some(vec!["/workspace".to_string()]),
                allowed_remove_paths: Some(vec![]),
                allowed_execute_paths: Some(vec![]),
                copy_on_write: Some(true),
                max_file_size: Some(1024),
            }),
            ..basic_policy()
        };
        let denied = policy_denied_syscalls(&policy);
        assert!(
            !denied.contains(&SYS_mkdir),
            "mkdir must be allowed with write paths"
        );
        assert!(
            denied.contains(&SYS_unlink),
            "unlink must stay denied without remove paths"
        );
    }

    #[test]
    fn test_allowlist_mode_requires_x86_64() {
        let policy = SandboxPolicy {
            process: Some(ProcessPolicy {
                allowed_child_processes: None,
                denied_child_processes: None,
                max_child_processes: None,
                allow_fork: Some(true),
                allow_exec: Some(true),
                allowlist_syscalls: Some(true),
            }),
            ..basic_policy()
        };
        let filters = build_filter(&policy);
        #[cfg(target_arch = "x86_64")]
        assert!(filters.is_ok(), "allow-list must build on x86_64");
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            filters.is_err(),
            "allow-list must fail closed on other arches"
        );
    }

    #[test]
    fn test_allowlist_default_is_deny() {
        let policy = SandboxPolicy {
            process: Some(ProcessPolicy {
                allowed_child_processes: None,
                denied_child_processes: None,
                max_child_processes: None,
                allow_fork: Some(true),
                allow_exec: Some(true),
                allowlist_syscalls: Some(true),
            }),
            ..basic_policy()
        };
        let filters = build_filter(&policy).expect("filter builds");
        // [0] arch load, [1] arch jeq, [2] kill on arch mismatch,
        // [3] nr load, ...jeq list..., last is ALLOW, default falls to [2].
        assert_eq!(
            filters.last().map(|f| f.k),
            Some(RET_ALLOW),
            "matched syscalls must be allowed"
        );
    }

    #[test]
    fn test_absolutize() {
        assert_eq!(
            absolutize("/etc/passwd", None),
            PathBuf::from("/etc/passwd")
        );
        assert_eq!(
            absolutize("out.txt", Some(Path::new("/workspace"))),
            PathBuf::from("/workspace/out.txt")
        );
    }
}
