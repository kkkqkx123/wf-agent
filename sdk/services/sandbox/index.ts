/**
 * Sandbox Module — Unified Export
 *
 * Exports strategy implementations, runtime, resolver, default policies,
 * and shared types for the sandbox subsystem.
 */

// Runtime types
export type { SandboxExecutionResult } from "./types.js";

// Strategy implementations
export { ShellStaticAnalyzerStrategy } from "./strategies/shell-static-analyzer.js";
export {
  PythonBuiltinHookStrategy,
  PythonASTAnalyzerStrategy,
} from "./strategies/python-strategies/index.js";
export { JavaScriptVmContextStrategy } from "./strategies/js-vm-context.js";

// Phase 3: Runtime & Resolver
export { SandboxRuntime, getSandboxRuntime, resetSandboxRuntime } from "./sandbox-runtime.js";
export type { SandboxRuntimeResult } from "./sandbox-runtime.js";
export { DefaultStrategyResolver } from "./strategy-resolver.js";
export {
  DEFAULT_SANDBOX_POLICY,
  DEFAULT_SHELL_POLICY,
  DEFAULT_PYTHON_POLICY,
  DEFAULT_JS_POLICY,
  SHELL_POLICY_PRESETS,
} from "./default-policy.js";
