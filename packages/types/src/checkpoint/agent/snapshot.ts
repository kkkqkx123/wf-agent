/**
 * Agent Loop State Snapshot Type Definition
 */

import type { Message } from "../../message/index.js";
import { AgentLoopStatus } from "../../agent-execution/types.js";
import type { IterationRecord } from "../../agent-execution/types.js";

/**
 * Agent Loop Status Snapshot
 */
export interface AgentLoopStateSnapshot {
  /** state of affairs */
  status: AgentLoopStatus;
  /** Current number of iterations */
  currentIteration: number;
  /** Number of tool calls */
  toolCallCount: number;
  /** Starting time */
  startTime: number | null;
  /** end time */
  endTime: number | null;
  /** error message */
  error: unknown;
  /** Message History */
  messages: Message[];
  /** variable set */
  variables: Record<string, unknown>;
  /** deployment */
  config?: unknown;

  // ========== Extended fields for complete state capture ==========

  /** Iteration history records */
  iterationHistory?: IterationRecord[];
  /** Streaming flag */
  isStreaming?: boolean;
  /** Pending tool call IDs */
  pendingToolCalls?: string[];

  /** Allow additional properties for extensibility */
  [key: string]: unknown;
}
