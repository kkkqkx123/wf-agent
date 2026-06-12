/**
 * Linux Seccomp-bpf Strategy
 *
 * Provides syscall-level sandboxing via seccomp-loader (a native helper binary).
 * Falls back to passthrough when the binary is not found.
 */

import type { SandboxPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";
import { getTerminalService, type TerminalService } from "../../../terminal/index.js";
import { execSync } from "child_process";
import { existsSync } from "fs";
import { resolve } from "path";
import { executePassthrough, recordAudit } from "./base.js";

// Extended syscall categories for richer policy mapping
const SYSCALL_CATEGORIES = {
  fileIO: [
    "read",
    "write",
    "open",
    "openat",
    "close",
    "lseek",
    "pread64",
    "pwrite64",
    "readv",
    "writev",
    "sendfile",
    "truncate",
    "ftruncate",
    "fallocate",
  ],
  fsMeta: [
    "stat",
    "lstat",
    "fstat",
    "newfstatat",
    "statx",
    "access",
    "faccessat",
    "getdents",
    "getdents64",
    "readlink",
    "readlinkat",
    "name_to_handle_at",
  ],
  fsMod: [
    "mkdir",
    "mkdirat",
    "rmdir",
    "unlink",
    "unlinkat",
    "rename",
    "renameat",
    "renameat2",
    "symlink",
    "symlinkat",
    "link",
    "linkat",
    "mknod",
    "mknodat",
    "chmod",
    "fchmod",
    "chmodat",
    "chown",
    "fchown",
    "chownat",
    "utimensat",
    "futimesat",
  ],
  process: [
    "fork",
    "vfork",
    "clone",
    "clone3",
    "execve",
    "execveat",
    "exit",
    "exit_group",
    "wait4",
    "waitid",
    "getpid",
    "getppid",
    "gettid",
  ],
  memory: [
    "mmap",
    "munmap",
    "mprotect",
    "brk",
    "sbrk",
    "mlock",
    "munlock",
    "mlockall",
    "munlockall",
  ],
  network: [
    "socket",
    "connect",
    "bind",
    "listen",
    "accept",
    "accept4",
    "sendto",
    "recvfrom",
    "sendmsg",
    "recvmsg",
    "sendmmsg",
    "recvmmsg",
    "shutdown",
    "getsockopt",
    "setsockopt",
    "getsockname",
    "getpeername",
    "epoll_create",
    "epoll_ctl",
    "epoll_wait",
    "select",
    "poll",
    "ppoll",
  ],
  time: ["nanosleep", "clock_gettime", "gettimeofday", "time", "clock_nanosleep", "timer_create"],
  sync: ["futex", "futex_time64", "sync", "fsync", "fdatasync", "msync", "sync_file_range"],
  misc: [
    "ioctl",
    "fcntl",
    "flock",
    "dup",
    "dup2",
    "dup3",
    "pipe",
    "pipe2",
    "eventfd",
    "eventfd2",
    "signalfd",
    "userfaultfd",
    "memfd_create",
    "inotify_init",
    "inotify_add_watch",
  ],
} as const;

export class LinuxSeccompStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "linux-seccomp";
  name = "Linux Seccomp (OS Hook)";
  description = "Linux seccomp-bpf system call filtering (requires native seccomp-loader binary)";
  priority = 50;

  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    return process.platform === "linux";
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    if (!this.isAvailable()) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "LinuxSeccompStrategy is only available on Linux",
      };
    }

    // Try to use seccomp-loader binary for BPF filtering.
    const loaderPath = this.findSeccompLoader();
    if (!loaderPath) {
      recordAudit({
        timestamp: startTime,
        strategyId: this.id,
        command: options.command,
        allowed: true,
        reason: "seccomp-loader binary not found, falling back to passthrough",
      });
      // Fall back to passthrough when seccomp-loader binary not found
      return executePassthrough(this.terminalService, options, startTime);
    }

    // Build seccomp arguments from policy
    const policyArgs = this.buildPolicyArgs(policy);

    // Record audit of applied policy
    recordAudit({
      timestamp: startTime,
      strategyId: this.id,
      command: options.command,
      allowed: true,
      reason: `seccomp policy: deny=[${policyArgs.deny.join(",")}] allow=[${policyArgs.allow.join(",")}]`,
    });

    // Construct the full command: seccomp-loader --allow ... --deny ... -- <user-command>
    const allowList = policyArgs.allow.length > 0 ? `--allow ${policyArgs.allow.join(",")}` : "";
    const denyList = policyArgs.deny.length > 0 ? `--deny ${policyArgs.deny.join(",")}` : "";

    const seccompCommand = [loaderPath, allowList, denyList, "--", options.command]
      .filter(Boolean)
      .join(" ");

    try {
      const result = await this.terminalService.executeOneOff(seccompCommand, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-os-hook",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Search for the seccomp-loader binary in common locations.
   * Returns the absolute path if found, or null.
   */
  private findSeccompLoader(): string | null {
    const candidatePaths = [
      resolve(process.cwd(), "bin", "seccomp-loader"),
      resolve(process.cwd(), "target", "release", "seccomp-loader"),
      resolve(process.cwd(), "target", "debug", "seccomp-loader"),
    ];

    // Also check system PATH via execSync
    try {
      const which = execSync("which seccomp-loader", {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
        timeout: 1000,
      }).trim();
      if (which) {
        candidatePaths.unshift(which);
      }
    } catch {
      // Not on PATH — use compiled paths only
    }

    for (const p of candidatePaths) {
      if (existsSync(p)) {
        return p;
      }
    }
    return null;
  }

  /**
   * Map SandboxPolicy to seccomp --allow / --deny syscall lists.
   *
   * Uses SYSCALL_CATEGORIES for granular control based on policy fields:
   * - filesystem.writeable vs readable: control fsMod syscalls
   * - process.allowExec: control exec* syscalls
   * - process.allowFork: control fork/clone syscalls
   * - network.access: control network syscalls
   *
   * Returns { allow: string[], deny: string[] } where each element
   * is a syscall name (e.g. "read", "write", "execve").
   */
  private buildPolicyArgs(policy: SandboxPolicy): { allow: string[]; deny: string[] } {
    const allow = new Set<string>([
      // Essential for any program to run
      ...SYSCALL_CATEGORIES.fileIO,
      ...SYSCALL_CATEGORIES.fsMeta,
      ...SYSCALL_CATEGORIES.memory,
      ...SYSCALL_CATEGORIES.time,
      ...SYSCALL_CATEGORIES.sync,
      ...SYSCALL_CATEGORIES.misc,

      // Process basics (exit, getpid, wait)
      "exit",
      "exit_group",
      "getpid",
      "getppid",
      "gettid",
      "wait4",
      "waitid",
    ]);

    const deny = new Set<string>();

    // Process policy: deny exec/fork syscalls
    if (policy.process?.allowExec === false) {
      deny.add("execve");
      deny.add("execveat");
    }
    if (policy.process?.allowFork === false) {
      deny.add("fork");
      deny.add("vfork");
      deny.add("clone");
      deny.add("clone3");
    }

    // Network policy: deny all network syscalls
    if (policy.network?.access === "none") {
      for (const syscall of SYSCALL_CATEGORIES.network) {
        deny.add(syscall);
      }
    }

    // Filesystem policy: restrict modify operations when no writable paths are configured
    if (policy.filesystem?.allowedWritePaths?.length === 0) {
      // Deny only modification syscalls, allow reads
      for (const syscall of SYSCALL_CATEGORIES.fsMod) {
        deny.add(syscall);
      }
    }

    // Custom denylist via resource extensions
    if (policy.resource?.timeoutLimit) {
      // timeoutLimit is handled at the resolver layer
    }

    return {
      allow: Array.from(allow).filter(s => !deny.has(s)),
      deny: Array.from(deny),
    };
  }
}
