/**
 * Default Sandbox Policy
 *
 * Provides safe default values for all sandbox sub-policies.
 * Architecture reference: docs/infra/sandbox/architecture.md
 */

import type { SandboxPolicy, ShellPolicy, PythonPolicy, JavaScriptPolicy, FilesystemPolicy, ProcessPolicy, NetworkPolicy, ResourcePolicy } from "@wf-agent/types";

export const DEFAULT_SHELL_POLICY: ShellPolicy = {
  allowedCommands: [],
  deniedCommands: [
    "sudo",
    "su",
    "chroot",
    "mount",
    "umount",
    "dd",
    "mkfs",
    "mkfs.ext4",
    "mkfs.xfs",
    "reboot",
    "shutdown",
    "poweroff",
    "halt",
    "passwd",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "chown",
    "chmod",
    "insmod",
    "rmmod",
    "modprobe",
  ],
  dangerousPatterns: [
    "rm\\s+(-rf|--recursive)\\s+\\/\\s*",
    ":\\(\\)\\s*\\{.*:\\(\\)\\s*\\}.*\\}",
    "\\|\\s*(bash|sh|zsh)\\s",
    "curl.*\\|.*(bash|sh)",
    "wget.*\\|.*(bash|sh)",
    "LD_PRELOAD=",
    "PYTHONPATH=.*:",
    "\\/dev\\/(null|zero|random|urandom)",
    "fork\\s*\\(\\)\\s*\\{",
    "exec\\s+\\/s?bin\\/(init|systemd)",
  ],
  allowPipe: true,
  allowRedirect: true,
};

export const DEFAULT_PYTHON_POLICY: PythonPolicy = {
  allowedModules: [],
  deniedModules: [
    "os",
    "subprocess",
    "shutil",
    "ctypes",
    "socket",
    "pty",
    "signal",
    "multiprocessing",
    "distutils",
    "sysconfig",
  ],
  allowSubprocess: false,
  restrictBuiltinOpen: true,
  allowDynamicEval: false,
};

export const DEFAULT_JS_POLICY: JavaScriptPolicy = {
  allowedModules: [],
  deniedModules: [
    "child_process",
    "cluster",
    "worker_threads",
    "v8",
    "vm",
    "inspector",
    "module",
    "process",
  ],
  allowChildProcess: false,
  allowFSWrite: false,
  allowDynamicEval: false,
};

export const DEFAULT_FILESYSTEM_POLICY: FilesystemPolicy = {
  allowedReadPaths: [],
  allowedWritePaths: [],
  allowedRemovePaths: [],
  allowedExecutePaths: [],
  copyOnWrite: false,
  maxFileSize: 10 * 1024 * 1024,
};

export const DEFAULT_PROCESS_POLICY: ProcessPolicy = {
  allowedChildProcesses: [],
  deniedChildProcesses: [],
  maxChildProcesses: 0,
  allowFork: false,
  allowExec: false,
};

export const DEFAULT_NETWORK_POLICY: NetworkPolicy = {
  access: "none",
  allowDns: false,
};

export const DEFAULT_RESOURCE_POLICY: ResourcePolicy = {
  timeoutLimit: 30000,
};

export const DEFAULT_SANDBOX_POLICY: SandboxPolicy = {
  mode: "strict",
  shell: DEFAULT_SHELL_POLICY,
  python: DEFAULT_PYTHON_POLICY,
  javascript: DEFAULT_JS_POLICY,
  filesystem: DEFAULT_FILESYSTEM_POLICY,
  process: DEFAULT_PROCESS_POLICY,
  network: DEFAULT_NETWORK_POLICY,
  resource: DEFAULT_RESOURCE_POLICY,
};