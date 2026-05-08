/**
 * Terminal related type definitions
 */

/**
 * Terminal configuration options
 */
export interface TerminalOptions {
  /** Shell path, which is automatically selected based on the platform by default. */
  shell?: string;
  /** Working directory */
  cwd?: string;
  /** Environment Variables */
  env?: Record<string, string>;
  /** Number of columns in the terminal */
  cols?: number;
  /** Number of terminal lines */
  rows?: number;
  /** Whether to run in the background (without displaying a terminal window) */
  background?: boolean;
  /** Output log file path (used during background operations) */
  logFile?: string;
}

/**
 * Terminal session
 */
export interface TerminalSession {
  /** Session Unique Identifier */
  id: string;
  /** Pseudo-terminal instance */
  pty: any;
  /** Process ID */
  pid: number;
  /** Creation time */
  createdAt: Date;
  /** Session State */
  status: "active" | "inactive" | "closed";
}

/**
 * Task execution result
 */
export interface TaskExecutionResult {
  /** Task Unique Identifier */
  taskId: string;
  /** Terminal session ID */
  sessionId: string;
  /** Task Status */
  status: "started" | "running" | "completed" | "failed";
  /** Start time */
  startTime: Date;
  /** End time */
  endTime?: Date;
  /** Output content */
  output?: string;
  /** Error message */
  error?: string;
}

/**
 * Task status
 */
export interface TaskStatus {
  /** Task Unique Identifier */
  taskId: string;
  /** Status */
  status: "running" | "completed" | "failed" | "cancelled";
  /** Progress percentage (0-100) */
  progress?: number;
  /** Status messages */
  message?: string;
  /** Last update time */
  lastUpdate: Date;
}

/**
 * Bridge message type
 */
export interface BridgeMessage {
  /** Message Type */
  type: "status" | "output" | "error" | "command";
  /** Message payload */
  payload: any;
  /** timestamp */
  timestamp: Date;
}

/**
 * Terminal event
 */
export interface TerminalEvent {
  /** Event Type */
  type: "data" | "exit" | "error";
  /** Event data */
  data?: string;
  /** Exit code */
  exitCode?: number;
  /** Exit signal */
  signal?: number;
  /** Error object */
  error?: Error;
}
