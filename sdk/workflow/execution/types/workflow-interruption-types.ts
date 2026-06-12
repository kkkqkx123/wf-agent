/**
 * Workflow Interruption Types
 *
 * Workflow-specific interruption exception types.
 * These extend the core interruption types with workflow-specific context.
 */

import { InterruptedException } from "../../../core/types/interruption-types.js";
import type { InterruptionType } from "../../../core/types/interruption-types.js";

/**
 * Workflow Execution Interrupt Exception
 *
 * Extends InterruptedException with workflow-specific context (executionId, nodeId).
 */
export class WorkflowExecutionInterruptedException extends InterruptedException {
  constructor(
    message: string,
    interruptionType: InterruptionType,
    public readonly executionId?: string,
    public readonly nodeId?: string,
    context?: Record<string, unknown>,
  ) {
    super(message, interruptionType, { ...context, executionId, nodeId });
  }
}
