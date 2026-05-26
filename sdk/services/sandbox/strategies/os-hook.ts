/**
 * OS-level Hook Strategies — Skeleton Implementations (Phase 6)
 *
 * Architecture reference: docs/infra/sandbox/strategies/os-level-hook.md
 *
 * OS Hook strategies provide platform-level isolation as an enhancement layer.
 * They do NOT replace lightweight strategies but stack on top of them.
 *
 * Current status: skeleton with platform detection.
 * Full implementations require native bindings:
 *   - Linux seccomp-bpf: C/rust seccomp-loader binary
 *   - Windows Job Object: koffi/edge-js FFI bindings
 *   - Proot path redirect: ptrace helper or proot binary
 */

import type { SandboxPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";

// =========================================================================
// 1. Linux Seccomp-bpf Strategy
// =========================================================================

/**
 * Linux seccomp-bpf sandbox strategy.
 *
 * Available: Linux only with kernel seccomp support.
 * Implementation requires a native helper binary (seccomp-loader)
 * that applies BPF rules before executing the user command.
 */
export class LinuxSeccompStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "os-hook";
  name = "Linux Seccomp (OS Hook)";
  description = "Linux seccomp-bpf system call filtering (requires native seccomp-loader binary)";
  priority = 50;

  isAvailable(): boolean {
    return process.platform === "linux";
  }

  async execute(
    _options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
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

    return {
      success: false,
      scriptName: "sandbox-os-hook",
      executionTime: Date.now() - startTime,
      error: "LinuxSeccompStrategy requires seccomp-loader binary — not yet implemented",
    };
  }
}

// =========================================================================
// 2. Windows Job Object Strategy
// =========================================================================

/**
 * Windows Job Object sandbox strategy.
 *
 * Available: Windows only.
 * Implementation requires FFI bindings (koffi/edge-js) to call Win32 API:
 *   - CreateJobObject / SetInformationJobObject / AssignProcessToJobObject
 */
export class WindowsJobObjectStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "os-hook";
  name = "Windows Job Object (OS Hook)";
  description = "Windows Job Object process group isolation (requires FFI bindings)";
  priority = 50;

  isAvailable(): boolean {
    return process.platform === "win32";
  }

  async execute(
    _options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    if (!this.isAvailable()) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "WindowsJobObjectStrategy is only available on Windows",
      };
    }

    return {
      success: false,
      scriptName: "sandbox-os-hook",
      executionTime: Date.now() - startTime,
      error: "WindowsJobObjectStrategy requires native FFI bindings — not yet implemented",
    };
  }
}

// =========================================================================
// 3. Proot-like Path Redirect Strategy
// =========================================================================

/**
 * Proot-style path redirection strategy.
 *
 * Available: Linux (ptrace) / Windows (Detours).
 * Redirects file system paths for all child processes transparently.
 * Useful when combined with OverlayVFS for CoW in external tools.
 */
export class ProotLikeRedirectStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "os-hook";
  name = "Proot Path Redirect (OS Hook)";
  description = "Path redirection using ptrace or DLL injection for transparent VFS";
  priority = 40;

  isAvailable(): boolean {
    return process.platform === "linux";
  }

  async execute(
    _options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    if (!this.isAvailable()) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "ProotLikeRedirectStrategy requires ptrace support — only available on Linux",
      };
    }

    return {
      success: false,
      scriptName: "sandbox-os-hook",
      executionTime: Date.now() - startTime,
      error: "ProotLikeRedirectStrategy requires proot binary or ptrace helper — not yet implemented",
    };
  }
}