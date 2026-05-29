/**
 * Python Builtin-hook Strategy
 *
 * Wraps Python code with secure builtins before execution via subprocess.
 * Replaces builtins.open, builtins.__import__, and disables dangerous modules.
 *
 * Architecture reference: docs/infra/sandbox/strategies/python-sandbox.md
 */

import type { SandboxPolicy, PythonPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../types.js";
import { getTerminalService, type TerminalService } from "../../terminal/index.js";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

/**
 * Default denied Python modules when policy.python.deniedModules is empty.
 */
export const DEFAULT_DENIED_MODULES: string[] = [
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
];

/**
 * Python Builtin-hook Strategy
 *
 * Generates a wrapped Python script that:
 *   - Clears sys.path (isolates from system packages)
 *   - Replaces builtins.open with a safe version (checks write path whitelist)
 *   - Replaces builtins.__import__ with a safe version (module whitelist/blacklist)
 *   - Disables dangerous pre-loaded modules (os, subprocess, shutil, etc.)
 */
export class PythonBuiltinHookStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "builtin-hook";
  name = "Python Builtin Hook";
  description = "Python script sandboxing by hooking builtins and restricting module imports";
  priority = 20;

  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    return this.checkPythonAvailable();
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const code = options.command;
    const pyPolicy: PythonPolicy = {
      allowedModules: policy.python?.allowedModules ?? [],
      deniedModules: policy.python?.deniedModules ?? DEFAULT_DENIED_MODULES,
      allowSubprocess: policy.python?.allowSubprocess ?? false,
      restrictBuiltinOpen: policy.python?.restrictBuiltinOpen ?? true,
      allowDynamicEval: policy.python?.allowDynamicEval ?? false,
    };

    if (!code) {
      return {
        success: false,
        scriptName: "sandbox-python",
        executionTime: Date.now() - startTime,
        error: "Empty Python code",
      };
    }

    const vfsEnabled = !!options.vfs;
    const writableDirs = vfsEnabled && options.cwd ? [options.cwd] : [];
    const wrappedCode = this.wrapWithSandbox(code, pyPolicy, vfsEnabled, writableDirs);
    const tmpFile = path.join(os.tmpdir(), `sandbox-py-${Date.now()}-${Math.random().toString(36).slice(2, 8)}.py`);

    try {
      fs.writeFileSync(tmpFile, wrappedCode, "utf-8");
      const result = await this.terminalService.executeOneOff(
        `python "${tmpFile}"`,
        {
          cwd: options.cwd,
          env: {
            ...options.env,
            PYTHONPATH: "",
            PYTHONDONTWRITEBYTECODE: "1",
            PYTHONSTARTUP: "",
            PYTHONHOME: "",
          },
          timeout: options.timeout ?? policy.resource?.timeoutLimit,
        },
      );

      return {
        success: result.success,
        scriptName: "sandbox-python",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-python",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    } finally {
      try {
        fs.unlinkSync(tmpFile);
      } catch {
        // Ignore cleanup errors
      }
    }
  }

  /**
   * Generate a wrapped Python script with security hooks.
   * When VFS is enabled, write operations are allowed within the workspace
   * directory (the VFS delta layer on the Node.js side handles CoW).
   */
  private wrapWithSandbox(code: string, policy: PythonPolicy, vfsEnabled: boolean = false, writableDirs: string[] = []): string {
    const deniedModulesJson = JSON.stringify(policy.deniedModules);
    const allowedModulesJson = JSON.stringify(policy.allowedModules);
    const restrictOpen = policy.restrictBuiltinOpen ? "True" : "False";
    const allowSubprocess = policy.allowSubprocess ? "True" : "False";
    const writableDirsJson = JSON.stringify(writableDirs);

    return `
import sys
import builtins as __builtins_module
import os as _os

# ── Clear system path (unless in whitelist mode, where allowed modules need import paths) ──
if not ${allowedModulesJson}:
    sys.path = []

# ── Disable dangerous pre-loaded modules ──
for _mod in ${deniedModulesJson}:
    sys.modules[_mod] = None

# ── Safe __import__ with module whitelist/blacklist ──
_DENIED_MODULES = set(${deniedModulesJson})
_ALLOWED_MODULES = set(${allowedModulesJson})
_DENIED_BUILTINS = {"eval", "exec", "compile"}

def _safe_import(name, *args, **kwargs):
    if _ALLOWED_MODULES and name not in _ALLOWED_MODULES:
        raise ImportError(f"Module not allowed: {name}")
    if name in _DENIED_MODULES:
        raise ImportError(f"Module denied: {name}")
    return __builtins_module.__import__(name, *args, **kwargs)

__builtins_module.__import__ = _safe_import

# ── Disable dangerous builtins ──
for _b in _DENIED_BUILTINS:
    try:
        setattr(__builtins_module, _b, None)
    except TypeError:
        pass

# ── Safe open - restrict write operations ──
# When VFS is enabled (vfsEnabled=${vfsEnabled}), writes are allowed
# within the workspace directory (${writableDirsJson}) because the VFS
# delta layer on the Node.js side provides copy-on-write isolation.
_WRITABLE_DIRS = set(${writableDirsJson})
_VFS_ENABLED = ${vfsEnabled ? "True" : "False"}

if ${restrictOpen}:
    _ORIGINAL_OPEN = __builtins_module.open
    def _safe_open(file, mode='r', *args, **kwargs):
        if 'w' in mode or 'a' in mode or 'x' in mode or '+' in mode:
            if _VFS_ENABLED:
                # VFS mode: allow writes within writable directories
                abs_path = _os.path.abspath(file)
                for _wdir in _WRITABLE_DIRS:
                    if abs_path.startswith(_os.path.abspath(_wdir)):
                        return _ORIGINAL_OPEN(file, mode, *args, **kwargs)
                raise PermissionError(f"Write denied (outside workspace): {file}")
            else:
                raise PermissionError(f"Write denied: {file}")
        return _ORIGINAL_OPEN(file, mode, *args, **kwargs)
    __builtins_module.open = _safe_open

# ── Disable subprocess if not allowed ──
if not ${allowSubprocess}:
    sys.modules["subprocess"] = None

# ════════════════════════════════════════
# User code follows
# ════════════════════════════════════════
${code}
`;
  }

  /**
   * Check if Python is available on the system.
   */
  private checkPythonAvailable(): boolean {
    try {
      const { spawnSync } = require("child_process");
      const result = spawnSync("python", ["--version"], {
        timeout: 5000,
        stdio: "pipe",
        encoding: "utf-8",
      });
      return result.status === 0;
    } catch {
      return false;
    }
  }
}