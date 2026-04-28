/**
 * Agent Hook Handler Module Export
 *
 * Responsibilities:
 * - Provide unified export for Agent Hook handlers
 * - Export hook execution functions and context builders
 * - Export event emission utilities
 *
 * Supported Hook Types:
 * - BEFORE_ITERATION: Before iteration starts
 * - AFTER_ITERATION: After iteration ends
 * - BEFORE_TOOL_CALL: Before tool call starts
 * - AFTER_TOOL_CALL: After tool call ends
 * - BEFORE_LLM_CALL: Before LLM call starts
 * - AFTER_LLM_CALL: After LLM call ends
 */

export {
  executeAgentHook,
  type AgentHookExecutionContext,
  type AgentHookDefinition,
} from "./hook-handler.js";

export {
  buildAgentHookEvaluationContext,
  convertToEvaluationContext,
  type AgentHookEvaluationContext,
} from "./context-builder.js";

export { emitAgentHookEvent, type AgentCustomEventData } from "./event-emitter.js";
