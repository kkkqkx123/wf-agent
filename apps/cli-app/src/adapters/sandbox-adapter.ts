/**
 * Sandbox Policy Management Adapter
 * Encapsulates sandbox runtime and security policy inspection
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  getSandboxRuntime,
  DEFAULT_SANDBOX_POLICY,
  DEFAULT_SHELL_POLICY,
  DEFAULT_PYTHON_POLICY,
  DEFAULT_JS_POLICY,
  SHELL_POLICY_PRESETS,
} from "@wf-agent/sdk/services";

/**
 * Sandbox Adapter
 * Provides CLI-friendly access to sandbox runtime and policy information
 */
export class SandboxAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get sandbox runtime instance
   */
  getRuntime() {
    return getSandboxRuntime();
  }

  /**
   * Get sandbox runtime status overview
   */
  async getStatus(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const runtime = this.getRuntime();
      return {
        runtime: "SandboxRuntime",
        enabled: runtime.isEnabled(),
        strategies: ["ShellStaticAnalyzerStrategy", "PythonBuiltinHookStrategy", "PythonASTAnalyzerStrategy", "JavaScriptVmContextStrategy"],
      };
    }, "Get sandbox runtime status");
  }

  /**
   * Get the default sandbox policy (complete)
   */
  async getDefaultPolicy(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return DEFAULT_SANDBOX_POLICY as unknown as Record<string, unknown>;
    }, "Get default sandbox policy");
  }

  /**
   * Get shell security policy details
   */
  async getShellPolicy(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return {
        allowedCommands: DEFAULT_SHELL_POLICY.allowedCommands,
        deniedCommands: DEFAULT_SHELL_POLICY.deniedCommands,
        dangerousPatterns: DEFAULT_SHELL_POLICY.dangerousPatterns,
        allowPipe: DEFAULT_SHELL_POLICY.allowPipe,
        allowRedirect: DEFAULT_SHELL_POLICY.allowRedirect,
      };
    }, "Get shell sandbox policy");
  }

  /**
   * Get Python security policy details
   */
  async getPythonPolicy(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return {
        deniedModules: DEFAULT_PYTHON_POLICY.deniedModules,
        allowedModules: DEFAULT_PYTHON_POLICY.allowedModules,
        allowSubprocess: DEFAULT_PYTHON_POLICY.allowSubprocess,
        restrictBuiltinOpen: DEFAULT_PYTHON_POLICY.restrictBuiltinOpen,
        allowDynamicEval: DEFAULT_PYTHON_POLICY.allowDynamicEval,
      };
    }, "Get Python sandbox policy");
  }

  /**
   * Get JavaScript security policy details
   */
  async getJsPolicy(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return {
        deniedModules: DEFAULT_JS_POLICY.deniedModules,
      };
    }, "Get JavaScript sandbox policy");
  }

  /**
   * Get all shell policy presets (SAFE, BALANCED, PERMISSIVE)
   */
  async getPresets(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return {
        SAFE: {
          description: "Most restrictive — allowlist mode, no pipe/redirect",
          allowedCommands: SHELL_POLICY_PRESETS.SAFE.allowedCommands ?? [],
          deniedCommands: (SHELL_POLICY_PRESETS.SAFE.deniedCommands ?? []).length,
          allowPipe: SHELL_POLICY_PRESETS.SAFE.allowPipe ?? false,
          allowRedirect: SHELL_POLICY_PRESETS.SAFE.allowRedirect ?? false,
        },
        BALANCED: {
          description: "Denylist-based with no redirect, pipe allowed",
          deniedCommands: (SHELL_POLICY_PRESETS.BALANCED.deniedCommands ?? []).length,
          allowPipe: SHELL_POLICY_PRESETS.BALANCED.allowPipe ?? false,
          allowRedirect: SHELL_POLICY_PRESETS.BALANCED.allowRedirect ?? false,
        },
        PERMISSIVE: {
          description: "Most permissive — only critical commands denied",
          deniedCommands: SHELL_POLICY_PRESETS.PERMISSIVE.deniedCommands ?? [],
          allowPipe: SHELL_POLICY_PRESETS.PERMISSIVE.allowPipe ?? false,
          allowRedirect: SHELL_POLICY_PRESETS.PERMISSIVE.allowRedirect ?? false,
        },
      };
    }, "Get shell policy presets");
  }

  /**
   * Get available strategies
   */
  async getStrategies(): Promise<string[]> {
    return this.executeWithErrorHandling(async () => {
      return [
        "ShellStaticAnalyzerStrategy — 47 denied commands, 9 dangerous patterns",
        "PythonBuiltinHookStrategy — 10 denied modules (os, subprocess, ctypes, etc.)",
        "PythonASTAnalyzerStrategy — AST-level code analysis",
        "JavaScriptVmContextStrategy — 7 denied modules (child_process, worker_threads, etc.)",
        "DefaultStrategyResolver — routes to strategy based on language",
      ];
    }, "Get sandbox strategies");
  }
}
