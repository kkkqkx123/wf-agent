/**
 * Sandbox Type Definitions
 * Policy declarations, strategy identifiers, and runtime configuration for script sandboxing
 *
 * Architecture reference:
 *   docs/infra/sandbox/architecture.md  §4 (Type Design)
 *   docs/infra/sandbox/strategies/vfs-overlay.md  §VFS Config
 */

import type { ScriptRiskLevel } from "./script-security.js";

// ============================================================================
// Sandbox Mode (§4.1)
// ============================================================================

/**
 * Sandbox mode
 * - disabled: completely off (default)
 * - lenient: log warnings only, never reject
 * - strict: strict mode, reject violations
 * - custom: fully controlled by policy
 */
export type SandboxMode = "disabled" | "lenient" | "strict" | "custom";

// ============================================================================
// Script Language (§4.4)
// ============================================================================

/**
 * Script language type for sandbox routing
 */
export type ScriptLanguage = "auto" | "shell" | "python" | "javascript";

// ============================================================================
// Policy System (§4.2)
// ============================================================================

export interface FilesystemPolicy {
  /** Glob patterns for readable paths */
  allowedReadPaths: string[];
  /** Glob patterns for writable paths */
  allowedWritePaths: string[];
  /** Glob patterns for removable paths */
  allowedRemovePaths: string[];
  /** Glob patterns for executable paths */
  allowedExecutePaths: string[];
  /** Copy-on-write mode */
  copyOnWrite: boolean;
  /** Max file size in bytes */
  maxFileSize: number;
}

export interface ProcessPolicy {
  /** Whitelist of allowed child processes */
  allowedChildProcesses: string[];
  /** Blacklist of denied child processes */
  deniedChildProcesses: string[];
  /** Maximum number of child processes */
  maxChildProcesses: number;
  /** Allow forking */
  allowFork: boolean;
  /** Allow exec */
  allowExec: boolean;
}

export interface NetworkPolicy {
  /** Network access level */
  access: "none" | "localhost" | "specific" | "all";
  /** Allowed domain list */
  allowedDomains?: string[];
  /** Allowed port ranges */
  allowedPorts?: [number, number][];
  /** Allow DNS resolution */
  allowDns: boolean;
}

export interface ResourcePolicy {
  /** CPU time limit in ms */
  cpuLimit?: number;
  /** Memory limit in MB */
  memoryLimit?: number;
  /** Disk usage limit in MB */
  diskLimit?: number;
  /** Timeout in ms */
  timeoutLimit: number;
}

export interface ShellPolicy {
  /** Whitelist of allowed commands */
  allowedCommands: string[];
  /** Blacklist of denied commands */
  deniedCommands: string[];
  /** Dangerous pattern regex array */
  dangerousPatterns: string[];
  /** Allow pipe operator */
  allowPipe: boolean;
  /** Allow redirect operator */
  allowRedirect: boolean;
}

export interface PythonPolicy {
  /** Whitelist of allowed modules */
  allowedModules: string[];
  /** Blacklist of denied modules */
  deniedModules: string[];
  /** Allow subprocess module */
  allowSubprocess: boolean;
  /** Restrict builtins.open to allowed paths */
  restrictBuiltinOpen: boolean;
  /** Allow dynamic eval/exec */
  allowDynamicEval: boolean;
}

export interface JavaScriptPolicy {
  /** Whitelist of allowed modules */
  allowedModules: string[];
  /** Blacklist of denied modules */
  deniedModules: string[];
  /** Allow child_process module */
  allowChildProcess: boolean;
  /** Allow filesystem write operations */
  allowFSWrite: boolean;
  /** Allow dynamic eval/Function */
  allowDynamicEval: boolean;
}

/**
 * Sandbox policy declaration
 * Each sub-policy is optional; defaults are applied at runtime.
 */
export interface SandboxPolicy {
  mode: SandboxMode;
  filesystem?: Partial<FilesystemPolicy>;
  process?: Partial<ProcessPolicy>;
  network?: Partial<NetworkPolicy>;
  resource?: Partial<ResourcePolicy>;
  shell?: Partial<ShellPolicy>;
  python?: Partial<PythonPolicy>;
  javascript?: Partial<JavaScriptPolicy>;
}

// ============================================================================
// Strategy System (§4.3)
// ============================================================================

/** Shell sandbox strategy identifiers */
export type ShellSandboxStrategy = "static-analyzer" | "os-hook" | "container" | "custom";

/** Python sandbox strategy identifiers */
export type PythonSandboxStrategy =
  | "builtin-hook"
  | "ast-analyzer"
  | "os-hook"
  | "pyodide-wasm"
  | "container"
  | "custom";

/** JavaScript sandbox strategy identifiers */
export type JavaScriptSandboxStrategy =
  | "vm-context"
  | "isolated-vm"
  | "os-hook"
  | "container"
  | "custom";

import type { ExecutorShellConfig, ScriptRuntime, RuntimeConfig } from "./script-executor.js";

/**
 * Minimal options passed into strategy execution
 */
export interface StrategyExecuteOptions {
  /** Command / script content to execute */
  command: string;
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Timeout in milliseconds */
  timeout?: number;
  /** Target shell type, set by executor from Script.executor.shell */
  shellType?: ExecutorShellConfig;
  /** Execution runtime environment, set by executor from Script.executor.runtime */
  runtime?: ScriptRuntime;
  /** Runtime-specific connection config (docker/ssh), set by executor from Script.executor.runtimeConfig */
  runtimeConfig?: RuntimeConfig;
}

/**
 * Strategy implementation contract
 * Implement this interface to provide a custom sandbox strategy.
 */
export interface StrategyImplementation<TResult> {
  /** Unique strategy identifier */
  id: string;
  /** Human-readable name */
  name: string;
  /** Short description */
  description: string;
  /** Priority (higher = preferred first) */
  priority: number;
  /** Execute the command with sandbox policy applied */
  execute(options: StrategyExecuteOptions, policy: SandboxPolicy): Promise<TResult>;
  /** Check if this strategy is available in the current environment */
  isAvailable(): boolean;
}

/**
 * Strategy resolver contract
 * Resolves strategy identifiers to concrete implementations.
 */
export interface StrategyResolver {
  /** Resolve a shell strategy by id */
  resolveShellStrategy(id: ShellSandboxStrategy | string): StrategyImplementation<unknown>;
  /** Resolve a python strategy by id */
  resolvePythonStrategy(id: PythonSandboxStrategy | string): StrategyImplementation<unknown>;
  /** Resolve a javascript strategy by id */
  resolveJavaScriptStrategy(
    id: JavaScriptSandboxStrategy | string,
  ): StrategyImplementation<unknown>;
  /** Register a custom strategy implementation */
  registerStrategy(
    language: "shell" | "python" | "javascript",
    impl: StrategyImplementation<unknown>,
  ): void;
}

// ============================================================================
// VFS Config
// ============================================================================

export interface VFSConfig {
  /** Enable VFS overlay */
  enabled: boolean;
  /** Storage backend */
  storage: "memory" | "sqlite";
  /** Workspace root path */
  workspaceRoot: string;
  /** SQLite database path (sqlite mode only) */
  dbPath?: string;
  /** Path access policy */
  pathPolicy?: {
    readable?: string[];
    writable?: string[];
  };
}

// ============================================================================
// Sandbox Profile (application-level config)
// ============================================================================

/**
 * Named sandbox profile — defined once in application config,
 * referenced by name at execution time.
 */
export interface SandboxProfile {
  /** Profile name (referenced by SandboxConfig.profile or SandboxProfileRule.profile) */
  name: string;
  /** Human-readable description */
  description?: string;
  /** Sandbox mode */
  mode: SandboxMode;
  /** Shell strategy priority list (fallback order) */
  shellStrategy?: (ShellSandboxStrategy | string)[];
  /** Python strategy priority list (fallback order) */
  pythonStrategy?: (PythonSandboxStrategy | string)[];
  /** JavaScript strategy priority list (fallback order) */
  javascriptStrategy?: (JavaScriptSandboxStrategy | string)[];
  /** Policy declarations */
  policy?: Partial<SandboxPolicy>;
  /** VFS configuration */
  vfs?: VFSConfig;
}

/**
 * Profile selection rule — matches a script by language and/or risk level
 * to a named profile. Evaluated in order; first match wins.
 */
export interface SandboxProfileRule {
  /** Match on script language (single or multiple) */
  language?: ScriptLanguage | ScriptLanguage[];
  /** Match on script risk level (single or multiple) */
  riskLevel?: ScriptRiskLevel | ScriptRiskLevel[];
  /** Target profile name (must match a SandboxProfile.name) */
  profile: string;
}

/**
 * Global sandbox configuration — typically loaded from an application config file.
 * Defines profiles and auto-matching rules that apply across all script executions.
 */
export interface SandboxGlobalConfig {
  /** Default sandbox mode for all scripts */
  mode?: SandboxMode;
  /** Named profiles */
  profiles?: SandboxProfile[];
  /** Profile selection rules (evaluated in order) */
  rules?: SandboxProfileRule[];
  /** Fallback profile when no rules match */
  defaultProfile?: string;
}

// ============================================================================
// Sandbox Config (§4.5 - Execution-time overrides)
// ============================================================================

/**
 * Execution-time sandbox configuration.
 * All fields are optional overrides on top of the resolved profile.
 *
 * Resolution order at runtime:
 *   1. SandboxGlobalConfig.defaultProfile (or mode)
 *   2. SandboxProfileRule matching (language + riskLevel)
 *   3. SandboxConfig.profile (explicit reference)
 *   4. SandboxConfig inline overrides (mode / policy / strategy / vfs)
 */
export interface SandboxConfig {
  /** Reference a predefined SandboxProfile by name */
  profile?: string;

  /** Override sandbox mode (overrides profile's mode) */
  mode?: SandboxMode;

  /** Inline policy overrides on top of the resolved profile */
  policy?: Partial<SandboxPolicy>;

  /** Override shell strategy priority list */
  shellStrategy?: (ShellSandboxStrategy | string)[];
  /** Override python strategy priority list */
  pythonStrategy?: (PythonSandboxStrategy | string)[];
  /** Override javascript strategy priority list */
  javascriptStrategy?: (JavaScriptSandboxStrategy | string)[];

  /** Custom strategy resolver (fully overrides built-in resolver) */
  customProvider?: StrategyResolver;

  /** Override VFS configuration */
  vfs?: VFSConfig;

  // ========================================================================
  // Legacy backward-compatible fields
  // Mapped to the new config format at runtime by SandboxRuntime.
  // ========================================================================

  /** @deprecated Use mode + policy instead */
  type?: "docker" | "nodejs" | "python" | "custom";
  /** @deprecated Container image identifier */
  image?: string;
  /** @deprecated Use resource policy instead */
  resourceLimits?: {
    memory?: number;
    cpu?: number;
    disk?: number;
  };
  /** @deprecated Use network policy instead */
  network?: {
    enabled: boolean;
    allowedDomains?: string[];
  };
  /** @deprecated Use filesystem policy instead */
  filesystem?: {
    allowedPaths?: string[];
    readOnly?: boolean;
  };
}
