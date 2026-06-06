/**
 * Agent Message Types
 *
 * Messages related to Agent Loop lifecycle, iterations, LLM interaction, and tools.
 */

/**
 * Agent Message Type
 */
export const AgentMessageType = {
  // Lifecycle
  AGENT_START: "agent.start",
  AGENT_PAUSE: "agent.pause",
  AGENT_RESUME: "agent.resume",
  AGENT_END: "agent.end",
  AGENT_CANCEL: "agent.cancel",
  // Iteration
  ITERATION_START: "agent.iteration.start",
  ITERATION_END: "agent.iteration.end",
  ITERATION_LIMIT: "agent.iteration.limit",
  // LLM Interaction
  LLM_REQUEST: "agent.llm.request",
  LLM_STREAM: "agent.llm.stream",
  LLM_RESPONSE: "agent.llm.response",
  LLM_ERROR: "agent.llm.error",
  // Tool Execution
  TOOL_CALL_START: "agent.tool.call_start",
  TOOL_CALL_END: "agent.tool.call_end",
  TOOL_RESULT: "agent.tool.result",
  TOOL_ERROR: "agent.tool.error",
  // Checkpoint
  CHECKPOINT_CREATE: "agent.checkpoint.create",
  CHECKPOINT_RESTORE: "agent.checkpoint.restore",
  // Message History
  MESSAGE_ADD: "agent.message.add",
} as const;

/**
 * Agent Message Type
 */
export type AgentMessageType = typeof AgentMessageType[keyof typeof AgentMessageType];

/**
 * Agent Start Data
 */
export interface AgentStartData {
  /** Agent loop ID */
  loopId: string;

  /** Agent ID (profile) */
  agentId: string;

  /** Agent configuration */
  config: {
    /** Maximum iterations */
    maxIterations: number;

    /** Tool IDs */
    tools: string[];

    /** System prompt */
    systemPrompt?: string;

    /** Model ID */
    modelId?: string;
  };
}

/**
 * Agent End Data
 */
export interface AgentEndData {
  /** Agent loop ID */
  loopId: string;

  /** Final status */
  status: "completed" | "failed" | "cancelled" | "limit_reached";

  /** Final result */
  result?: unknown;

  /** Error message (if failed) */
  error?: string;

  /** Total iterations */
  totalIterations: number;

  /** Total duration in milliseconds */
  duration: number;
}

/**
 * Agent Iteration Data
 *
 * Used for iteration-related component messages.
 * Note: Iteration status is conveyed via message type (ITERATION_START vs ITERATION_END),
 * not via a status field. This follows the event-driven pattern where event types
 * indicate state transitions.
 */
export interface AgentIterationData {
  /** Current iteration number (1-based) */
  iteration: number;

  /** Tool call count in this iteration */
  toolCallCount?: number;

  /** Iteration duration in milliseconds (only available when completed) */
  duration?: number;
}

/**
 * Agent LLM Request Data
 */
export interface AgentLLMRequestData {
  /** Request ID */
  requestId: string;

  /** Message count sent to LLM */
  messageCount: number;

  /** Tool count available */
  toolCount: number;
}

/**
 * Agent LLM Stream Data
 */
export interface AgentLLMStreamData {
  /** Stream chunk content */
  chunk: string;

  /** Whether this is the final chunk */
  isComplete: boolean;

  /** Message ID (for accumulating chunks) */
  messageId: string;
}

/**
 * Agent LLM Response Data
 */
export interface AgentLLMResponseData {
  /** Response ID */
  responseId: string;

  /** Full response content */
  content: string;

  /** Thinking content (if any) */
  thinking?: string;

  /** Tool calls (if any) */
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>;

  /** Token usage */
  usage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}

/**
 * Agent LLM Error Data
 */
export interface AgentLLMErrorData {
  /** Error code */
  code: string;

  /** Error message */
  message: string;

  /** Whether retry is possible */
  retryable: boolean;
}

/**
 * Agent Tool Call Data
 */
export interface AgentToolCallData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Tool arguments */
  arguments: Record<string, unknown>;

  /** Brief summary for display */
  summary: string;
}

/**
 * Agent Tool End Data
 */
export interface AgentToolEndData {
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
 * Agent Tool Result Data
 */
export interface AgentToolResultData {
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
 * Agent Tool Error Data
 */
export interface AgentToolErrorData {
  /** Tool call ID */
  toolCallId: string;

  /** Tool name */
  toolName: string;

  /** Error message */
  error: string;

  /** Error code */
  code?: string;
}

/**
 * Agent Checkpoint Data
 */
export interface AgentCheckpointData {
  /** Checkpoint ID */
  checkpointId: string;

  /** Agent loop ID */
  loopId: string;

  /** Iteration number */
  iteration: number;

  /** Checkpoint path */
  path?: string;
}

/**
 * Agent Message Add Data
 */
export interface AgentMessageAddData {
  /** Message ID */
  messageId: string;

  /** Message role */
  role: "user" | "assistant" | "system" | "tool";

  /** Message content (truncated for display) */
  contentPreview: string;

  /** Full content length */
  contentLength: number;
}
