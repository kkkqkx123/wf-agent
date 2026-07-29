use std::os::unix::process::{CommandExt, ExitStatusExt};

use async_trait::async_trait;
use wf_types::script::sandbox::{NetworkAccessType, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

// ============== Seccomp BPF filter builder ==============

fn bpf_stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt: 0, jf: 0, k }
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

fn build_deny_filter(denied: &[i64]) -> Vec<libc::sock_filter> {
    let n = denied.len();
    let mut filters = Vec::with_capacity(n + 2);

    filters.push(bpf_stmt(0x20, 0));

    for (i, &nr) in denied.iter().enumerate() {
        let offset_to_kill = n - i;
        filters.push(bpf_jump(0x15, nr as u32, offset_to_kill as u8, 0));
    }

    filters.push(bpf_stmt(0x06, 0x7fff0000));
    filters.push(bpf_stmt(0x06, 0x80000000));
    filters
}

fn get_denied_syscalls(policy: &SandboxPolicy) -> Vec<i64> {
    use libc::*;
    let mut denied: Vec<i64> = vec![
        SYS_ptrace,                  // 101
        SYS_process_vm_readv,        // 310
        SYS_process_vm_writev,       // 311
        SYS_bpf,                     // 321
        SYS_kexec_file_load,         // 320
        SYS_kexec_load,              // 246
        SYS_swapon,                  // 167
        SYS_swapoff,                 // 168
        SYS_init_module,             // 175
        SYS_delete_module,           // 176
        SYS_finit_module,            // 313
        SYS_iopl,                    // 172
        SYS_ioperm,                  // 173
        SYS_chroot,                  // 161
        SYS_modify_ldt,              // 154
        SYS_pivot_root,              // 155
        SYS_mount,                   // 165
        SYS_umount2,                 // 166
        SYS_syslog,                  // 103
        SYS_sethostname,             // 170
        SYS_setdomainname,           // 171
        SYS_reboot,                  // 169
        SYS_perf_event_open,         // 298
        SYS_quotactl,                // 179
        SYS_add_key,                 // 248
        SYS_request_key,             // 249
        SYS_keyctl,                  // 250
        SYS_lookup_dcookie,          // 212
        SYS_acct,                    // 163
        SYS_vhangup,                 // 153
        SYS_clock_settime,           // 227
        SYS_settimeofday,            // 164
        SYS_adjtimex,                // 159
        SYS_setuid,                  // 105
        SYS_setgid,                  // 106
        SYS_setreuid,                // 113
        SYS_setregid,                // 114
        SYS_setresuid,               // 117
        SYS_setresgid,               // 119
        SYS_setfsuid,                // 122
        SYS_setfsgid,                // 123
        SYS_setgroups,               // 116
        SYS_setpgid,                 // 109
        SYS_setsid,                  // 112
    ];

    let network_allowed = policy
        .network
        .as_ref()
        .map(|n| n.access_type != NetworkAccessType::None)
        .unwrap_or(false);

    if !network_allowed {
        denied.extend_from_slice(&[
            SYS_socket,      // 41
            SYS_connect,     // 42
            SYS_accept,      // 43
            SYS_sendto,      // 44
            SYS_recvfrom,    // 45
            SYS_sendmsg,     // 46
            SYS_recvmsg,     // 47
            SYS_shutdown,    // 48
            SYS_bind,        // 49
            SYS_listen,      // 50
            SYS_getsockname, // 51
            SYS_getpeername, // 52
            SYS_socketpair,  // 53
            SYS_setsockopt,  // 54
            SYS_getsockopt,  // 55
            SYS_accept4,     // 288
            SYS_sendmmsg,    // 307
            SYS_recvmmsg,    // 299
        ]);
    }

    denied.sort();
    denied.dedup();
    denied
}

#[cfg(target_os = "linux")]
pub struct LinuxSeccompStrategy;

#[cfg(target_os = "linux")]
#[async_trait]
impl StrategyImplementation for LinuxSeccompStrategy {
    fn id(&self) -> &str {
        "os-hook"
    }

    fn name(&self) -> &str {
        "Linux Seccomp (OS Hook)"
    }

    fn description(&self) -> &str {
        "Linux seccomp-bpf system call filtering with policy-driven deny list"
    }

    fn priority(&self) -> i32 {
        50
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let allowed = get_denied_syscalls(policy);
        let mut filters = build_deny_filter(&allowed);

        let cmd = options.command.clone();
        let result = tokio::task::spawn_blocking(move || -> std::io::Result<ScriptExecutionResult> {
            let started = std::time::Instant::now();
            let mut child = std::process::Command::new("sh");
            child.args(["-c", &cmd]);

            unsafe {
                child.pre_exec(move || {
                    let ret = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                    if ret < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    let prog = libc::sock_fprog {
                        len: filters.len() as u16,
                        filter: filters.as_mut_ptr(),
                    };
                    let ret = libc::syscall(
                        libc::SYS_seccomp,
                        libc::SECCOMP_SET_MODE_FILTER,
                        0i32,
                        &prog as *const libc::sock_fprog,
                    );
                    if ret < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }

            let output = child.output()?;
            let elapsed = started.elapsed().as_millis() as u64;

            let (exit_code, error_msg) = if let Some(code) = output.status.code() {
                (
                    Some(code),
                    if code == 0 {
                        None
                    } else {
                        Some(format!("Command failed with exit code {code}"))
                    },
                )
            } else if let Some(sig) = output.status.signal() {
                let reason = if sig == 9 || sig == 31 {
                    "Process killed by seccomp: system call denied by policy"
                } else {
                    "Process terminated by signal"
                };
                (Some(-sig), Some(reason.to_string()))
            } else {
                (None, Some("Process exited abnormally".to_string()))
            };

            Ok(ScriptExecutionResult {
                success: output.status.success(),
                script_name: "sandbox-os-hook".to_string(),
                stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
                stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
                exit_code,
                execution_time: elapsed,
                error: error_msg,
                sandbox_mode: None,
                strategy_id: Some("os-hook".to_string()),
                violations: None,
            })
        })
        .await
        .map_err(|e| format!("Task join error: {e}"))?
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

        Ok(result)
    }
}

#[cfg(not(target_os = "linux"))]
pub struct LinuxSeccompStrategy;

#[cfg(not(target_os = "linux"))]
#[async_trait]
impl StrategyImplementation for LinuxSeccompStrategy {
    fn id(&self) -> &str {
        "os-hook"
    }

    fn name(&self) -> &str {
        "Linux Seccomp (OS Hook)"
    }

    fn description(&self) -> &str {
        "Linux seccomp-bpf system call filtering (unavailable on this platform)"
    }

    fn priority(&self) -> i32 {
        50
    }

    fn is_available(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        _options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("seccomp is not available on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::SandboxMode;

    fn basic_policy() -> SandboxPolicy {
        SandboxPolicy {
            mode: SandboxMode::Strict,
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

    #[tokio::test]
    async fn test_seccomp_echo_works() {
        let strategy = LinuxSeccompStrategy;
        let policy = basic_policy();
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "echo hello seccomp".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        if cfg!(target_os = "linux") {
            assert!(result.success, "echo should work: {:?}", result.stderr);
            assert!(
                result
                    .stdout
                    .unwrap_or_default()
                    .contains("hello seccomp")
            );
        }
    }

    #[test]
    fn test_seccomp_deny_list_contains_dangerous_syscalls() {
        let policy = basic_policy();
        let denied = get_denied_syscalls(&policy);
        assert!(denied.contains(&101), "ptrace (101) must be in deny list");
        assert!(denied.contains(&321), "bpf (321) must be in deny list");
        assert!(denied.contains(&167), "swapon (167) must be in deny list");
        assert!(denied.contains(&169), "reboot (169) must be in deny list");
        assert!(denied.contains(&165), "mount (165) must be in deny list");
        assert!(denied.contains(&161), "chroot (161) must be in deny list");
        assert!(denied.contains(&175), "init_module (175) must be in deny list");
        assert!(denied.contains(&310), "process_vm_readv (310) must be in deny list");
        assert!(denied.contains(&311), "process_vm_writev (311) must be in deny list");
    }

    #[tokio::test]
    async fn test_seccomp_ls_works() {
        let strategy = LinuxSeccompStrategy;
        let policy = basic_policy();
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "ls /".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        if cfg!(target_os = "linux") {
            assert!(result.success, "ls should work: {:?}", result.stderr);
            let stdout = result.stdout.unwrap_or_default();
            assert!(
                stdout.contains("bin") || stdout.contains("usr") || stdout.contains("etc"),
                "ls output should contain standard dirs: {stdout}"
            );
        }
    }

    #[test]
    fn test_seccomp_deny_list_includes_network_when_disabled() {
        let policy = basic_policy();
        let denied = get_denied_syscalls(&policy);
        assert!(denied.contains(&41), "socket (41) must be in deny list when network disabled");
        assert!(denied.contains(&42), "connect (42) must be in deny list when network disabled");
        assert!(denied.contains(&44), "sendto (44) must be in deny list when network disabled");
        assert!(denied.contains(&45), "recvfrom (45) must be in deny list when network disabled");
    }

    #[test]
    fn test_seccomp_deny_list_excludes_network_when_enabled() {
        use wf_types::script::sandbox::NetworkPolicy;
        let policy = SandboxPolicy {
            network: Some(NetworkPolicy {
                access_type: NetworkAccessType::All,
                allowed_domains: None,
                allowed_ports: None,
                allow_dns: true,
            }),
            ..basic_policy()
        };
        let denied = get_denied_syscalls(&policy);
        assert!(!denied.contains(&41), "socket (41) must NOT be in deny list when network enabled");
    }
}
