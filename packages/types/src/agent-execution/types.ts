/**
 * Agent Loop Execution Basic Types
 *
 * This module contains fundamental type definitions for Agent Loop execution:
 * - Status enumeration
 * - Execution records (tool calls, iterations)
 * - Execution result
 *
 * Part of the agent-execution package for runtime-related types.
 */

// =============================================================================
// Status Enumeration
// =============================================================================

/**
 * Agent Loop Execution Status Enumeration
 */
export enum AgentLoopStatus {
  /** Created, not started */
  CREATED = "CREATED",
  /** Currently executing */
  RUNNING = "RUNNING",
  /** Paused (can be resumed) */
  PAUSED = "PAUSED",
  /** Completed successfully */
  COMPLETED = "COMPLETED",
  /** Failed with error */
  FAILED = "FAILED",
  /** Cancelled by user or system */
  CANCELLED = "CANCELLED",
}

// =============================================================================
// Execution Records
// =============================================================================

/**
 * Tool Call Record
 *
 * Records the execution details of a single tool call.
 */
export interface ToolCallRecord {
  /** Tool call unique identifier */
  id: string;
  /** Tool name */
  name: string;
  /** Tool invocation arguments */
  arguments: unknown;
  /** Execution result (if successful) */
  result?: unknown;
  /** Error message (if failed) */
  error?: string;
  /** Execution start timestamp */
  startTime: number;
  /** Execution end timestamp */
  endTime?: number;
}

/**
 * Iteration Record
 *
 * Records the complete execution details of a single agent iteration.
 * An iteration consists of one LLM call and its associated tool executions.
 */
export interface IterationRecord {
  /** Iteration number (1-based) */
  iteration: number;
  /** Iteration start timestamp */
  startTime: number;
  /** Iteration end timestamp */
  endTime?: number;
  /** Tool calls made during this iteration */
  toolCalls: ToolCallRecord[];
  /** LLM response content */
  responseContent?: string;
}

// =============================================================================
// Execution Result
// =============================================================================

/**
 * Agent Loop Execution Result
 *
 * Represents the outcome of an agent loop execution.
 */
export interface AgentLoopResult {
  /** Whether execution completed successfully */
  success: boolean;
  /** Final response content from the agent */
  content?: string;
  /** Total number of iterations executed */
  iterations: number;
  /** Total number of tool calls made */
  toolCallCount: number;
  /** Error information (if failed) */
  error?: unknown;
  /** Agent Loop ID */
  agentLoopId?: string;
}
