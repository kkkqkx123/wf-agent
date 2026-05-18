/**
 * Workflow Interruption Types
 * 
 * Workflow-specific interruption exception types.
 * These extend the core interruption types with workflow-specific context.
 */

import { InterruptedException } from "../../../core/types/interruption-types.js";
import type { InterruptionType } from "../../../core/utils/interruption/interruption-state.js";

/**
 * Workflow Execution Interrupt Exception Type
 * 
 * @description
 * 1. Used to indicate that workflow execution is interrupted by user request (pause or stop)
 * 2. Inherits from InterruptedException, adding workflow-specific context
 * 3. After the executor catches this exception, it will handle it according to the interruption type
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
