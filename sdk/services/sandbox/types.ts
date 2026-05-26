/**
 * Sandbox Runtime Types
 * Re-exports type definitions from @wf-agent/types and defines
 * sandbox-specific execution result extensions.
 */

export type {
  SandboxMode,
  SandboxPolicy,
  SandboxConfig,
  SandboxProfile,
  SandboxProfileRule,
  SandboxGlobalConfig,
  FilesystemPolicy,
  ProcessPolicy,
  NetworkPolicy,
  ResourcePolicy,
  ShellPolicy,
  PythonPolicy,
  JavaScriptPolicy,
  ShellSandboxStrategy,
  PythonSandboxStrategy,
  JavaScriptSandboxStrategy,
  StrategyImplementation,
  StrategyResolver,
  StrategyExecuteOptions,
  VFSConfig,
  ScriptLanguage,
} from "@wf-agent/types";

export type { ScriptExecutionResult, ExecutorMode } from "@wf-agent/types";

/**
 * Sandbox execution result
 * Extends ScriptExecutionResult with sandbox-specific metadata.
 */
export interface SandboxExecutionResult {
  success: boolean;
  scriptName: string;
  stdout?: string;
  stderr?: string;
  exitCode?: number;
  executionTime: number;
  error?: string;
  /** Sandbox mode that was applied during execution */
  sandboxMode?: string;
  /** Strategy identifier that was used */
  strategyId?: string;
  /** List of policy violations (lenient mode only) */
  violations?: string[];
}
