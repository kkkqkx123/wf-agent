/**
 * Agent Hook Context Builder
 *
 * Responsible for building evaluation context for Agent Hook execution.
 * Referenced from Graph module's context-builder.ts design.
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { AgentStateCoordinator } from "../../../state-managers/agent-state-coordinator.js";
import type { EvaluationContext } from "@wf-agent/types";
import { getAvailableTools } from "@wf-agent/types";

/**
 * Agent Hook evaluation context (for internal use)
 */
export interface AgentHookEvaluationContext {
  /** Current iteration count */
  iteration: number;
  /** Maximum number of iterations */
  maxIterations: number;
  /** Number of tool calls */
  toolCallCount: number;
  /** Current status */
  status: string;
  /** The last error (if any) */
  error?: unknown;
  /** Configuration information */
  config: {
    profileId?: string;
    systemPrompt?: string;
    tools?: string[];
  };
  /** Current tool call information (available for BEFORE/AFTER_TOOL_CALL) */
  toolCall?: {
    id: string;
    name: string;
    arguments: unknown;
    result?: unknown;
    error?: string;
  };
  /** Tool management API */
  tools: {
    /** Check if tool is available */
    isAvailable: (toolId: string) => boolean;
    /** Get all available tools */
    getAll: () => Set<string>;
  };
  /** Conversation manager reference (for accessing messages) */
  conversationManager: {
    /** Get all messages including invisible ones */
    getAllMessages: () => import("@wf-agent/types").LLMMessage[];
    /** Get visible messages only */
    getMessages: () => import("@wf-agent/types").LLMMessage[];
  };
}

/**
 * Build Agent Hook evaluation context
 *
 * @param entity Agent Loop entity
 * @param stateCoordinator Agent State Coordinator for message access
 * @param toolCallInfo Tool call information (optional)
 * @returns Evaluation context
 */
export function buildAgentHookEvaluationContext(
  entity: AgentLoopEntity,
  stateCoordinator: AgentStateCoordinator,
  toolCallInfo?: {
    id: string;
    name: string;
    arguments: unknown;
    result?: unknown;
    error?: string;
  },
): AgentHookEvaluationContext {
  const { config, state } = entity;

  return {
    iteration: state.currentIteration,
    maxIterations: config.maxIterations ?? 10,
    toolCallCount: state.toolCallCount,
    status: state.status,
    error: state.error,
    config: {
      profileId: config.profileId,
      systemPrompt: config.systemPrompt,
      tools: getAvailableTools(config.availableTools),
    },
    toolCall: toolCallInfo,
    // Tool management API
    tools: {
      isAvailable: (toolId) => entity.isToolAvailable(toolId),
      getAll: () => entity.getAvailableTools(),
    },
    // Conversation manager reference for message access
    conversationManager: {
      getAllMessages: () => stateCoordinator.getAllMessages(),
      getMessages: () => stateCoordinator.getMessages(),
    },
  };
}

/**
 * Convert to EvaluationContext
 *
 * @param hookContext Agent Hook evaluation context
 * @returns EvaluationContext
 */
export function convertToEvaluationContext(
  hookContext: AgentHookEvaluationContext,
): EvaluationContext {
  // Extract messages from conversation manager
  const messages = hookContext.conversationManager.getAllMessages();
  const lastMessage = messages.length > 0 ? messages[messages.length - 1] : null;

  return {
    input: {
      iteration: hookContext.iteration,
      maxIterations: hookContext.maxIterations,
      toolCallCount: hookContext.toolCallCount,

      // NEW: Full message history for conditional logic
      messages: messages,

      // NEW: Convenience accessor for last message
      lastMessage: lastMessage,
    },
    output: {
      status: hookContext.status,
      error: hookContext.error,
    },
    variables: {}, // Agent Loop does not use scoped variables
  };
}
