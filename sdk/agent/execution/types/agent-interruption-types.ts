/**
 * Agent Interruption Types
 * 
 * Agent-specific interruption exception types.
 * These extend the core interruption types with agent-specific context (iteration).
 */

import { InterruptedException } from "../../../core/types/interruption-types.js";
import type { InterruptionType } from "../../../core/types/interruption-types.js";

/**
 * Agent Execution Interrupt Exception
 * 
 * Extends InterruptedException with agent-specific context (iteration, agentLoopId).
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
