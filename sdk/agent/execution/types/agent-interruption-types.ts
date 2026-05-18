/**
 * Agent Interruption Types
 * 
 * Agent-specific interruption exception types.
 * These extend the core interruption types with agent-specific context (iteration).
 */

import { InterruptedException } from "../../../core/types/interruption-types.js";
import type { InterruptionType } from "../../../core/utils/interruption/interruption-state.js";

/**
 * Agent Execution Interrupt Exception Type
 * 
 * @description
 * 1. Used to indicate that agent loop execution is interrupted by user request (pause or stop)
 * 2. Inherits from InterruptedException, adding agent-specific context (iteration, agentLoopId)
 * 3. After the executor catches this exception, it will handle it according to the interruption type
 */
export class AgentExecutionInterruptedException extends InterruptedException {
  constructor(
    message: string,
    interruptionType: InterruptionType,
    public readonly agentLoopId?: string,
    public readonly iteration?: number,
    context?: Record<string, unknown>,
  ) {
    super(message, interruptionType, { ...context, agentLoopId, iteration });
  }
}
