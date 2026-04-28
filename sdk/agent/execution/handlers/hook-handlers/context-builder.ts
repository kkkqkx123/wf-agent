/**
 * Agent Hook Context Builder
 *
 * Responsible for building evaluation context for Agent Hook execution.
 * Referenced from Graph module's context-builder.ts design.
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { EvaluationContext } from "@wf-agent/types";

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
  /** Current variable state */
  variables: Record<string, unknown>;
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
}

/**
 * Build Agent Hook evaluation context
 *
 * @param entity Agent Loop entity
 * @param toolCallInfo Tool call information (optional)
 * @returns Evaluation context
 */
export function buildAgentHookEvaluationContext(
  entity: AgentLoopEntity,
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
    variables: entity.getAllVariables(),
    config: {
      profileId: config.profileId,
      systemPrompt: config.systemPrompt,
      tools: config.tools,
    },
    toolCall: toolCallInfo,
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
  return {
    input: {
      iteration: hookContext.iteration,
      maxIterations: hookContext.maxIterations,
      toolCallCount: hookContext.toolCallCount,
    },
    output: {
      status: hookContext.status,
      error: hookContext.error,
    },
    variables: hookContext.variables,
  };
}
