/**
 * Agent Error Handler
 *
 * Responsibilities:
 * - Handle errors during Agent Loop execution
 * - Unified error normalization and context building
 * - Integrate error handling tool functions for logging and triggering events
 * - Managing Agent Loop Status
 *
 * Design Principles:
 * - Functional implementation, statelessness
 * - Use stateless error handling tool functions
 * - severity-driven: stop execution at ERROR level only
 * - Consistent with error-handler.ts in the Graph module.
 */

import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ErrorContext, SDKError } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { SDKError as SDKErrorClass } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { handleError } from "../../../core/utils/error-utils.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { generateId, now } from "@wf-agent/common-utils";
import type { AgentPausedEvent, AgentCancelledEvent } from "@wf-agent/types";
import type { ExecutionInterruptionCheckResult } from "../../../core/utils/interruption/index.js";

const logger = createContextualLogger({ component: "AgentErrorHandler" });

/**
 * Building Agent error context
 *
 * @param entity Agent Loop entity
 * @param operation Operation type
 * @param additionalContext Additional context information
 * @returns Complete error context
 */
function buildAgentErrorContext(
  entity: AgentLoopEntity,
  operation: string,
  additionalContext?: Partial<ErrorContext>,
): ErrorContext {
  return {
    executionId: entity.id,
    nodeId: entity.nodeId,
    operation,
    iteration: entity.state.currentIteration,
    toolCallCount: entity.state.toolCallCount,
    ...additionalContext,
  };
}

/**
 * Standardize the error to SDKError
 *
 * @param error The original error
 * @param context The error context
 * @returns The standardized SDKError
 */
function standardizeAgentError(error: Error, context: ErrorContext): SDKError {
  // If it's already an SDKError, return it directly.
  if (error instanceof SDKErrorClass) {
    return error;
  }

  // Otherwise, wrap it as an SDKError, with the ERROR level being used by default.
  return new SDKErrorClass(error.message, "error", context, error);
}

/**
 * Determine whether the error is recoverable
 *
 * @param error: An SDKError object
 * @returns: Whether the error is recoverable
 */
function isRecoverableError(error: SDKError): boolean {
  // TimeoutError and ToolError are set to the warning level by default and can be recovered from.
  return error.severity === "warning" || error.severity === "info";
}

/**
 * Handling Agent Loop execution error
 *
 * @param entity Agent Loop instance
 * @param error Original error
 * @param operation Type of the operation
 * @param additionalContext Additional context information
 * @param eventManager Event manager
 * @returns Standardized error
 */
export async function handleAgentError(
  entity: AgentLoopEntity,
  error: Error,
  operation: string,
  additionalContext?: Partial<ErrorContext>,
  eventManager?: EventRegistry,
): Promise<SDKError> {
  // Build error context
  const context = buildAgentErrorContext(entity, operation, additionalContext);

  logger.debug("Handling Agent Loop error", {
    agentLoopId: entity.id,
    operation,
    errorMessage: error.message,
    iteration: entity.state.currentIteration,
  });

  // Standardized error
  const standardizedError = standardizeAgentError(error, context);

  logger.info("Agent Loop error standardized", {
    agentLoopId: entity.id,
    operation,
    severity: standardizedError.severity,
    recoverable: isRecoverableError(standardizedError),
  });

  // Use stateless error handling utility functions (logging and triggering events)
  await handleError(eventManager, standardizedError, {
    executionId: entity.id,
    workflowId: context.workflowId || "",
    nodeId: context.nodeId,
  });

  // Decide whether to stop execution based on the severity.
  if (standardizedError.severity === "error") {
    entity.state.fail(standardizedError);
    logger.info("Agent Loop execution failed due to error", {
      agentLoopId: entity.id,
      operation,
      errorMessage: standardizedError.message,
    });
  } else {
    logger.info("Agent Loop error is recoverable, continuing execution", {
      agentLoopId: entity.id,
      operation,
      severity: standardizedError.severity,
    });
  }
  // Automatic continuation of execution for WARNING and INFO levels.

  return standardizedError;
}

/**
 * Handle Agent Loop interruption
 *
 * Note: This function is called AFTER executeWithInterruptionHandling
 * has already detected the interruption. It only handles status updates
 * and event emission.
 *
 * @param entity Agent Loop entity
 * @param interruption Interruption check result (already determined)
 * @param operation Operation type
 * @param eventManager Event manager
 */
export async function handleAgentInterruption(
  entity: AgentLoopEntity,
  interruption: ExecutionInterruptionCheckResult,
  operation: string,
  eventManager?: EventRegistry,
): Promise<void> {
  logger.info("Agent Loop interruption detected", {
    agentLoopId: entity.id,
    operation,
    interruptionType: interruption.type,
    iteration: entity.state.currentIteration,
    // Enhanced context for better observability
    isStreaming: entity.state.isStreaming,
    pendingToolCalls: Array.from(entity.state.pendingToolCalls),
    streamMessageLength: entity.state.streamMessage?.content?.length || 0,
    toolCallCount: entity.state.toolCallCount,
    abortReason: entity.getAbortSignal()?.reason ? String(entity.getAbortSignal()!.reason) : undefined,
  });

  // Build error context
  const context = buildAgentErrorContext(entity, operation, {
    interruptionType: interruption.type,
  });

  // Generate an interrupt error (warning level, which does not prevent further execution).
  const interruptionError = new SDKErrorClass(
    interruption.type === "paused" ? "Execution paused" : "Execution cancelled",
    "warning",
    context,
  );

  // Use a stateless error handling utility function
  await handleError(eventManager, interruptionError, {
    executionId: entity.id,
    workflowId: context.workflowId || "",
    nodeId: context.nodeId,
  });

  // Update Agent Status
  if (interruption.type === "paused") {
    entity.state.pause();
    logger.info("Agent Loop paused", {
      agentLoopId: entity.id,
      operation,
      iteration: entity.state.currentIteration,
      // Enhanced context
      isStreaming: entity.state.isStreaming,
      pendingToolCalls: Array.from(entity.state.pendingToolCalls),
      streamMessagePreserved: !!entity.state.streamMessage,
    });

    // Emit pause event
    if (eventManager) {
      try {
        const pauseEvent: AgentPausedEvent = {
          id: generateId(),
          type: "AGENT_PAUSED",
          timestamp: now(),
          agentLoopId: entity.id,
          iteration: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          isStreaming: entity.state.isStreaming,
          pendingToolCalls: entity.state.pendingToolCalls.size,
          streamMessagePreserved: !!entity.state.streamMessage,
        };
        await emit(eventManager, pauseEvent);
      } catch (error) {
        logger.debug("Failed to emit AGENT_PAUSED event", { error });
      }
    }
  } else {
    entity.state.cancel();
    logger.info("Agent Loop cancelled", {
      agentLoopId: entity.id,
      operation,
      iteration: entity.state.currentIteration,
      // Enhanced context
      isStreaming: entity.state.isStreaming,
      pendingToolCalls: Array.from(entity.state.pendingToolCalls),
      toolCallCount: entity.state.toolCallCount,
    });

    // Emit cancel event
    if (eventManager) {
      try {
        const cancelEvent: AgentCancelledEvent = {
          id: generateId(),
          type: "AGENT_CANCELLED",
          timestamp: now(),
          agentLoopId: entity.id,
          iteration: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          isStreaming: entity.state.isStreaming,
          pendingToolCalls: entity.state.pendingToolCalls.size,
        };
        await emit(eventManager, cancelEvent);
      } catch (error) {
        logger.debug("Failed to emit AGENT_CANCELLED event", { error });
      }
    }
  }
}

/**
 * Determine whether it is a recoverable Agent error
 *
 * @param error: An SDKError object
 * @returns: Whether the error is recoverable
 */
export function isRecoverableAgentError(error: SDKError): boolean {
  return isRecoverableError(error);
}

/**
 * Agent creation failed.
 *
 * @param entity: Agent Loop entity
 * @param message: Error message
 * @param operation: Type of the operation
 * @param cause: Original cause of the error
 * @param severity: Severity of the error
 * @returns: SDKError instance
 */
export function createAgentExecutionError(
  entity: AgentLoopEntity,
  message: string,
  operation: string,
  cause?: Error,
  severity: "error" | "warning" | "info" = "error",
): SDKError {
  const context = buildAgentErrorContext(entity, operation);
  return new SDKErrorClass(message, severity, context, cause);
}
