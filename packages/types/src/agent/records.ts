/**
 * Agent Loop Execution Record Type Definition
 */

/**
 * Tool Call Logs
 */
export interface ToolCallRecord {
  /** Tool Call ID */
  id: string;
  /** Tool Name */
  name: string;
  /** invoke a parameter */
  arguments: unknown;
  /** Execution result */
  result?: unknown;
  /** error message */
  error?: string;
  /** Start time */
  startTime: number;
  /** end time */
  endTime?: number;
}

/**
 * Iteration log
 */
export interface IterationRecord {
  /** Iteration number */
  iteration: number;
  /** Start time */
  startTime: number;
  /** end time */
  endTime?: number;
  /** Tool Call Logs */
  toolCalls: ToolCallRecord[];
  /** LLM Response Content */
  responseContent?: string;
}
