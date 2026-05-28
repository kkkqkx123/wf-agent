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
import { executePassthrough } from "./base.js";

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
      // Fall back to passthrough when seccomp-loader binary not found
      return executePassthrough(this.terminalService, options, startTime);
    }

    // Build seccomp arguments from policy
    const policyArgs = this.buildPolicyArgs(policy);

    // Construct the full command: seccomp-loader --allow ... --deny ... -- <user-command>
    const allowList = policyArgs.allow.length > 0
      ? `--allow ${policyArgs.allow.join(",")}`
      : "";
    const denyList = policyArgs.deny.length > 0
      ? `--deny ${policyArgs.deny.join(",")}`
      : "";

    const seccompCommand = [
      loaderPath,
      allowList,
      denyList,
      "--",
      options.command,
    ].filter(Boolean).join(" ");

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
   * Returns { allow: string[], deny: string[] } where each element
   * is a syscall name (e.g. "read", "write", "execve").
   */
  private buildPolicyArgs(policy: SandboxPolicy): { allow: string[]; deny: string[] } {
    // Default allow-list: essential syscalls for basic program execution
    const allow = new Set([
      "read", "write", "open", "close", "stat", "lstat", "fstat",
      "mmap", "munmap", "mprotect", "brk", "exit_group", "exit",
      "lseek", "pread64", "pwrite64", "readv", "writev",
      "getdents", "getdents64", "ioctl",
      "futex", "nanosleep", "clock_gettime", "gettimeofday",
    ]);

    const deny = new Set<string>();

    // Process policy: if exec/fork is denied, block the syscalls
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

    // Network policy: if access is 'none', block network syscalls
    if (policy.network?.access === "none") {
      deny.add("socket");
      deny.add("connect");
      deny.add("bind");
      deny.add("listen");
      deny.add("accept");
      deny.add("accept4");
      deny.add("sendto");
      deny.add("recvfrom");
      deny.add("sendmsg");
      deny.add("recvmsg");
    }

    // File system policy: if resource limits exist, apply them
    if (policy.resource?.timeoutLimit) {
      // timeoutLimit is handled at the resolver layer, but we can
      // also enforce a syscall-level timeout via seccomp if needed.
    }

    return {
      allow: Array.from(allow),
      deny: Array.from(deny),
    };
  }
}
