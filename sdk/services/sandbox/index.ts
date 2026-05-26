/**
 * Sandbox Module — Unified Export (Phase 2)
 *
 * Exports strategy implementations and shared types for the sandbox subsystem.
 * Phase 3 will add SandboxRuntime, DefaultStrategyResolver, and default-policy.ts.
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