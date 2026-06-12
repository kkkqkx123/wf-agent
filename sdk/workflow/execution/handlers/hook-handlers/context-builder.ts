/**
 * Hook Context Builder
 *
 * Responsible for building evaluation context for Hook execution.
 */

import type { HookExecutionContext } from "./hook-handler.js";
import type { EvaluationContext } from "@wf-agent/types";

/**
 * Hook evaluation context (for internal use)
 */
export interface HookEvaluationContext {
  /** Workflow input data */
  workflowInput: Record<string, unknown>;
  /** Current node's output (from execution result) */
  nodeOutput?: unknown;
  /** Final workflow output */
  output: unknown;
  /** Message history */
  messages: unknown[];
  /** Node status */
  status: string;
  /** Execution time (ms) */
  executionTime: number;
  /** Error message (if any) */
  error?: unknown;
  /** Current variable state */
  variables: Record<string, unknown>;
  /** Node configuration */
  config: unknown;
  /** Node metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Build Hook evaluation context
 *
 * @param context Hook execution context
 * @returns Evaluation context
 */
export function buildHookEvaluationContext(context: HookExecutionContext): HookEvaluationContext {
  const { workflowExecutionEntity, node, result, conversationManager } = context;
  const workflowExecution = workflowExecutionEntity.getExecution();

  return {
    // NEW: Expose workflow input
    workflowInput: workflowExecutionEntity.getInput(),

    // NEW: Current node's output (from execution result)
    nodeOutput: result?.output,

    // Existing: Final workflow output
    output: workflowExecution.output,

    // Message history from ConversationSession (single data source)
    messages: conversationManager?.getMessages() || [],

    status: result?.status || "PENDING",
    executionTime: result?.executionTime || 0,
    error: result?.error,
    variables: workflowExecutionEntity.variableStateManager.getAllVariables(),
    config: node.config,
    metadata: node.metadata,
  };
}

/**
 * Convert to EvaluationContext
 *
 * @param hookContext Hook evaluation context
 * @returns EvaluationContext
 */
export function convertToEvaluationContext(hookContext: HookEvaluationContext): EvaluationContext {
  return {
    // NEW: Expose workflow input and messages
    input: {
      ...hookContext.workflowInput,
      messages: hookContext.messages,
    },
    output: {
      result: hookContext.output,
      nodeOutput: hookContext.nodeOutput, // NEW: Node-specific output
      status: hookContext.status,
      executionTime: hookContext.executionTime,
      error: hookContext.error,
    },
    variables: hookContext.variables,
  };
}
