/**
 * Sandbox Module — Unified Export
 *
 * Exports strategy implementations, runtime, resolver, default policies,
 * and shared types for the sandbox subsystem.
 */

// Runtime types
export type {
  SandboxExecutionResult,
} from "./types.js";

// Strategy implementations
export { ShellStaticAnalyzerStrategy } from "./strategies/shell-static-analyzer.js";
export { PythonBuiltinHookStrategy } from "./strategies/python-builtin-hook.js";
export { PythonASTAnalyzerStrategy } from "./strategies/python-ast-analyzer.js";
export { JavaScriptVmContextStrategy } from "./strategies/js-vm-context.js";

// Phase 3: Runtime & Resolver
export { SandboxRuntime, getSandboxRuntime, resetSandboxRuntime } from "./sandbox-runtime.js";
export type { SandboxRuntimeResult } from "./sandbox-runtime.js";
export { DefaultStrategyResolver } from "./strategy-resolver.js";
export { DEFAULT_SANDBOX_POLICY, DEFAULT_SHELL_POLICY, DEFAULT_PYTHON_POLICY, DEFAULT_JS_POLICY } from "./default-policy.js";
// Phase 3: Checkpoint VFS Bridge
export { CheckpointVFSBridge } from "./checkpoint-vfs-bridge.js";