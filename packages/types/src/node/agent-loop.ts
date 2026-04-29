/**
 * Agent Loop Node Type Definition
 *
 * Design principle: minimal configuration, focus on LLM-tools self-loop
 * No complex constraints, please use LOOP_START/LOOP_END + graph layout for complex control.
 */

import type { ID } from "../common.js";

/**
 * Agent Loop Node Configuration
 *
 * Agent Loop is used for simple workflow scenarios:
 * - Simple tasks with single/multiple rounds of tool calls
 * - Acts as the main orchestration engine to invoke sub workflows on-demand
 *
 * For complex control flows (conditional branching, state machines, etc.), use LOOP_START/LOOP_END + graph orchestration.
 */
export interface AgentLoopNodeConfig {
  /** LLM Placement ID */
  profileId: string;

  /**
   * Maximum number of iterations
   * @default 20
   * @description Force termination after this number of iterations, with or without tool calls.
   */
  maxIterations?: number;

  /**
   * List of available tools
   * @description If not specified, uses all available tools in the context
   */
  tools?: string[];

  /**
   * systemPromptTemplateId (specified directly, lower priority than systemPromptTemplateId)
   * @description If provided, it will be injected at each iteration of the
   */
  systemPrompt?: string;

  /**
   * System Prompt template ID (references a predefined template with higher priority than systemPrompt)
   * @description If provided, renders the system prompt via the template registry
   */
  systemPromptTemplateId?: string;

  /**
   * System Prompt Template Variable (required when systemPromptTemplateId is used)
   */
  systemPromptTemplateVariables?: Record<string, unknown>;
}

/**
 * Agent Loop Tool Call Log
 */
export interface AgentLoopToolCall {
  /** Number of iterations */
  iteration: number;
  /** Tool name */
  toolName: string;
  /** Tool Input */
  input: unknown;
  /** tool output */
  output: unknown;
  /** Tool Call ID */
  toolCallId: string;
}

/**
 * Agent Loop node execution results
 */
export interface AgentLoopNodeResult {
  /** Final LLM output content */
  content: string;
  /** Actual number of iterations */
  iterations: number;
  /** Whether to end because the maximum number of iterations has been reached */
  hitIterationLimit: boolean;
  /** History of tool calls used */
  toolCalls: AgentLoopToolCall[];
}

/**
 * Agent Loop node execution data
 * Used to pass data through the node processor
 */
export interface AgentLoopExecutionData {
  /** Node ID */
  nodeId: ID;
  /** Execution ID */
  executionId: ID;
  /** Enter the prompt */
  prompt?: string;
  /** Node Configuration */
  config: AgentLoopNodeConfig;
}
