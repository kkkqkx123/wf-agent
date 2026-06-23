/**
 * LLM Execution Configuration Types
 * Standardized configuration for LLM execution across modules (Graph, Agent, etc.)
 */

/**
 * LLM Execution Configuration
 * Common configuration interface for LLM execution
 */
export interface LLMExecutionConfig {
  /** LLM profile ID */
  profileId?: string;
  /** LLM parameters */
  parameters?: Record<string, unknown>;
  /** Maximum tool calls per request (default: 3) */
  maxToolCallsPerRequest?: number;
  /** Execution timeout (milliseconds) */
  timeout?: number;
  /** Enable token usage tracking */
  enableTokenTracking?: boolean;
  /** Token usage warning threshold (percentage, default: 80) */
  tokenWarningThreshold?: number;
  /** Token limit for warning */
  tokenLimit?: number;
}

/**
 * Workflow LLM Execution Config
 * Workflow-specific LLM execution configuration
 */
export interface GraphLLMExecutionConfig extends LLMExecutionConfig {
  /** Workflow ID */
  workflowId: string;
  /** Node ID */
  nodeId: string;
  /** Execution ID */
  executionId: string;
}

/**
 * Agent LLM Execution Config
 * Agent-specific LLM execution configuration
 */
export interface AgentLLMExecutionConfig extends LLMExecutionConfig {
  /** Agent ID */
  agentId: string;
  /** Session ID */
  sessionId: string;
}
