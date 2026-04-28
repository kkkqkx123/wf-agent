/**
 * Agent Message Types
 *
 * Messages related to Agent Loop lifecycle, iterations, LLM interaction, and tools.
 */

/**
 * Agent Message Type
 */
export enum AgentMessageType {
  // Lifecycle
  /** Agent started */
  START = "agent.start",

  /** Agent paused */
  PAUSE = "agent.pause",

  /** Agent resumed */
  RESUME = "agent.resume",

  /** Agent ended */
  END = "agent.end",

  /** Agent cancelled */
  CANCEL = "agent.cancel",

  // Iteration
  /** Iteration started */
  ITERATION_START = "agent.iteration.start",

  /** Iteration ended */
  ITERATION_END = "agent.iteration.end",

  /** Iteration limit reached */
  ITERATION_LIMIT = "agent.iteration.limit",

  // LLM Interaction
  /** LLM request sent */
  LLM_REQUEST = "agent.llm.request",

  /** LLM stream chunk received */
  LLM_STREAM = "agent.llm.stream",

  /** LLM response completed */
  LLM_RESPONSE = "agent.llm.response",

  /** LLM error */
  LLM_ERROR = "agent.llm.error",

  // Tool Execution
  /** Tool call started */
  TOOL_CALL_START = "agent.tool.call_start",

  /** Tool call ended */
  TOOL_CALL_END = "agent.tool.call_end",

  /** Tool result received */
  TOOL_RESULT = "agent.tool.result",

  /** Tool error */
  TOOL_ERROR = "agent.tool.error",

  // Human Relay
  /** Human relay request */
  HUMAN_RELAY_REQUEST = "agent.human_relay.request",

  /** Human relay response */
  HUMAN_RELAY_RESPONSE = "agent.human_relay.response",

  /** Human relay timeout */
  HUMAN_RELAY_TIMEOUT = "agent.human_relay.timeout",

  /** Human relay cancelled */
  HUMAN_RELAY_CANCEL = "agent.human_relay.cancel",

  // Checkpoint
  /** Checkpoint created */
  CHECKPOINT_CREATE = "agent.checkpoint.create",

  /** Checkpoint restored */
  CHECKPOINT_RESTORE = "agent.checkpoint.restore",

  // Message History
  /** Message added to history */
  MESSAGE_ADD = "agent.message.add",
}

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

  /** Parent thread ID (if called from Graph) */
  parentThreadId?: string;

  /** Node ID (if called from Graph) */
  nodeId?: string;
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
 */
export interface AgentIterationData {
  /** Current iteration number (1-based) */
  iteration: number;

  /** Maximum iterations */
  maxIterations: number;

  /** Tool call count in this iteration */
  toolCallCount: number;

  /** Message count in history */
  messageCount: number;

  /** Iteration status */
  status: "running" | "waiting" | "error";
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
 * Agent Human Relay Request Data
 */
export interface AgentHumanRelayRequestData {
  /** Request ID */
  requestId: string;

  /** Full prompt for human */
  prompt: string;

  /** Conversation context */
  context: {
    /** Recent messages */
    messages: Array<{
      role: "user" | "assistant" | "system";
      content: string;
      timestamp: number;
    }>;

    /** Additional metadata */
    metadata?: Record<string, unknown>;
  };

  /** Timeout in milliseconds */
  timeout: number;

  /** Output file path (for CLI) */
  outputFile?: string;

  /** Input file path (for CLI) */
  inputFile?: string;
}

/**
 * Agent Human Relay Response Data
 */
export interface AgentHumanRelayResponseData {
  /** Request ID */
  requestId: string;

  /** Human response content */
  content: string;

  /** Response time in milliseconds */
  responseTime: number;
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
