/**
 * Agent Loop execution result type definition
 */

/**
 * Agent loop results
 */
export interface AgentLoopResult {
  /** Did the execution succeed? */
  success: boolean;
  /** Final reply content */
  content?: string;
  /** Number of iterations */
  iterations: number;
  /** Number of tool calls recorded */
  toolCallCount: number;
  /** Error message */
  error?: unknown;
  /** Agent Loop ID */
  agentLoopId?: string;
}
