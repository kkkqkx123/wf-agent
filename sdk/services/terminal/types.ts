/**
 * Terminal Service Types
 * 
 * Type definitions for the terminal service that provides unified
 * terminal session management with multi-shell support.
 */

/**
 * Supported shell types
 */
export type ShellType =
  | "bash"
  | "zsh"
  | "fish"
  | "sh"
  | "cmd"
  | "powershell"
  | "pwsh"
  | "git-bash"
  | "wsl";

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
  /** Whether to enable auto-approval for commands */
  enableAutoApproval?: boolean;
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
  /** Whether to check auto-approval */
  checkApproval?: boolean;
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
  /** Whether auto-approval is enabled by default */
  enableAutoApproval?: boolean;
  /** Allowed commands for auto-approval */
  allowedCommands?: string[];
  /** Denied commands for auto-approval */
  deniedCommands?: string[];
  /** Maximum number of concurrent sessions */
  maxSessions?: number;
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
