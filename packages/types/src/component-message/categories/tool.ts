/**
 * Tool Message Types
 *
 * Messages related to tool execution (standalone, not within Agent context).
 */

/**
 * Tool Message Type
 */
export enum ToolMessageType {
  /** Tool call started */
  CALL_START = "tool.call_start",

  /** Tool call ended */
  CALL_END = "tool.call_end",

  /** Tool result received */
  RESULT = "tool.result",

  /** Tool error */
  ERROR = "tool.error",
}

/**
 * Tool Call Start Data
 */
export interface ToolCallStartData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Tool arguments */
  arguments: Record<string, unknown>;

  /** Brief summary for display */
  summary: string;

  /** Caller context */
  caller?: {
    type: "agent" | "graph" | "external";
    id: string;
  };
}

/**
 * Tool Call End Data
 */
export interface ToolCallEndData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Execution duration in milliseconds */
  duration: number;

  /** Whether successful */
  success: boolean;
}

/**
 * Tool Result Data
 */
export interface ToolResultData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Raw result */
  result: unknown;

  /** Formatted output for display */
  output: string;

  /** Brief summary for display */
  summary: string;

  /** Execution duration in milliseconds */
  duration: number;
}

/**
 * Tool Error Data
 */
export interface ToolErrorData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Error message */
  error: string;

  /** Error code */
  code?: string;

  /** Whether retry is possible */
  retryable?: boolean;
}
