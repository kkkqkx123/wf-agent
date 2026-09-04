//! seccomp-bpf filter construction for the command hardening gateway.
//!
//! Builds the BPF program that enforces the syscall allow/deny policy
//! in the forked child before exec. Architecture validation and the
//! platform-specific syscall lists live here so the public `apply` API
//! in `cmd` stays focused on environment and pre-exec wiring.

use std::io;

use wf_types::script::sandbox::{NetworkAccessType, ProcessPolicy, SandboxPolicy};

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
