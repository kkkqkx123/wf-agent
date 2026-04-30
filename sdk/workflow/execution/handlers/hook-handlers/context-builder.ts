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
  /** Node execution result */
  output: unknown;
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
  const { workflowExecutionEntity, node, result } = context;
  const thread = workflowExecutionEntity.getExecution();

  return {
    output: thread.output,
    status: result?.status || "PENDING",
    executionTime: result?.executionTime || 0,
    error: result?.error,
    variables: thread.variableScopes.workflowExecution,
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
    input: {},
    output: {
      result: hookContext.output,
      status: hookContext.status,
      executionTime: hookContext.executionTime,
      error: hookContext.error,
    },
    variables: hookContext.variables,
  };
}
