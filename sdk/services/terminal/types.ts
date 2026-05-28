/**
 * Terminal Service Types
 * 
 * Type definitions for the terminal service that provides unified
 * terminal session management with multi-shell support.
 */

import type { ShellType } from "@wf-agent/types";

// Re-export for consumers importing from @wf-agent/sdk/services
export type { ShellType };

/**
 * Terminal session status
 */
export type SessionStatus = "idle" | "busy" | "terminated";

/**
 * Terminal session configuration
 */
export interface TerminalSessionOptions {
  /** Shell type to use */
  shellType?: ShellType;
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Session timeout in milliseconds */
  timeout?: number;
  /** Associated task ID */
  taskId?: string;
}

/**
 * Terminal session state
 */
export interface TerminalSession {
  /** Unique session identifier */
  sessionId: string;
  /** Shell type */
  shellType: ShellType;
  /** Current working directory */
  cwd: string;
  /** Environment variables */
  env: Record<string, string>;
  /** Session status */
  status: SessionStatus;
  /** Creation timestamp */
  createdAt: number;
  /** Last activity timestamp */
  lastActiveAt: number;
  /** Associated task ID (optional) */
  taskId?: string;
  /** Output lines collected */
  outputLines: string[];
  /** Last read index for incremental output */
  lastReadIndex: number;
}

/**
 * Command execution options
 */
export interface ExecuteOptions {
  /** Command timeout in milliseconds */
  timeout?: number;
  /** Working directory override */
  cwd?: string;
  /** Environment variable override */
  env?: Record<string, string>;
}

/**
 * Command execution result
 */
export interface ExecuteResult {
  /** Whether execution succeeded */
  success: boolean;
  /** Standard output */
  stdout: string;
  /** Standard error */
  stderr: string;
  /** Exit code */
  exitCode: number;
  /** Error message if failed */
  error?: string;
  /** Session ID (for stateful execution) */
  sessionId?: string;
}

/**
 * Output retrieval options
 */
export interface OutputOptions {
  /** Regex filter pattern */
  filter?: string;
  /** Whether to include all output (not just new) */
  all?: boolean;
}

/**
 * Shell path overrides for custom installation paths.
 *
 * Keyed by ShellType, value is the custom executable path.
 * When set, overrides the default hardcoded path for that shell.
 */
export type ShellPathOverrides = Partial<Record<ShellType, string>>;

/**
 * Terminal service configuration
 */
export interface TerminalServiceConfig {
  /** Default shell type */
  defaultShellType?: ShellType;
  /** Default working directory */
  defaultCwd?: string;
  /** Default environment variables */
  defaultEnv?: Record<string, string>;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
  /** Maximum number of concurrent sessions */
  maxSessions?: number;
  /** Custom shell executable paths (overrides hardcoded defaults) */
  shellPathOverrides?: ShellPathOverrides;
}

/**
 * Shell information
 */
export interface ShellInfo {
  /** Shell type */
  type: ShellType;
  /** Executable path */
  path: string;
  /** Whether the shell is available on the system */
  available: boolean;
  /** Shell arguments for command execution */
  commandFlag: string;
}

/**
 * Options for spawning a process
 */
export interface ProcessSpawnOptions {
  /** Command to execute */
  command: string;
  /** Shell type override */
  shellType?: ShellType;
  /** Whether to use a shell (default: true) */
  shell?: boolean;
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Timeout in milliseconds */
  timeout?: number;
}

/**
 * Process information for a running command
 */
export interface ProcessInfo {
  /** Process ID */
  pid?: number;
  /** Command being executed */
  command: string;
  /** Start time */
  startTime: number;
  /** Whether the process is running */
  running: boolean;
}

/**
 * Terminal session with process information
 */
export interface TerminalSessionWithProcess extends TerminalSession {
  /** Current process information */
  process?: ProcessInfo;
}

/**
 * Event types for terminal service
 */
export interface TerminalServiceEvents {
  /** Session created */
  "session:created": [session: TerminalSession];
  /** Session terminated */
  "session:terminated": [sessionId: string];
  /** Command started */
  "command:started": [sessionId: string, command: string];
  /** Command completed */
  "command:completed": [sessionId: string, result: ExecuteResult];
  /** Output received */
  "output:received": [sessionId: string, line: string];
  /** Error occurred */
  "error": [error: Error, sessionId?: string];
}
