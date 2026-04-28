/**
 * Error Utilities - Functions for Error Handling
 * Provide stateless error handling capabilities
 *
 * Design Principles:
 * - Pure Functions: All methods are pure functions with no side effects.
 * - Stateless: Do not rely on DI containers; dependencies are passed in directly.
 * - Flexibility: The calling location can be customized as needed.
 */

import type { EventRegistry } from "../registry/event-registry.js";
import type { SDKError } from "@wf-agent/types";
import { buildErrorEvent } from "./event/builders/index.js";
import { safeEmit } from "./event/event-emitter.js";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * Standard error context
 * Defines the fields automatically extracted in error handling
 */
export interface StandardErrorContext {
  threadId?: string;
  workflowId?: string;
  nodeId?: string;
  agentLoopId?: string;
  iteration?: number;
  operation?: string;
  [key: string]: unknown;
}

/**
 * Automatically extract standard error context from entities
 *
 * @param source: Source of the context (WorkflowExecutionEntity, AgentLoopEntity, or a regular object)
 * @param operation: Name of the operation (optional)
 * @returns: Standard error context
 */
export function extractErrorContext(source: unknown, operation?: string): StandardErrorContext {
  const context: StandardErrorContext = { operation };

  if (!source || typeof source !== "object") {
    return context;
  }

  const src = source as Record<string, unknown>;

  // Extract WorkflowExecutionEntity-related fields
  if (src["id"] !== undefined) {
    // Use the threadId field preferentially, followed by the id.
    const threadId = src["threadId"];
    const id = src["id"];
    context.threadId =
      (typeof threadId === "string" ? threadId : null) ||
      (typeof id === "string" ? id : null) ||
      undefined;
  }

  // Extract workflowId (method or property)
  if (typeof src["getWorkflowId"] === "function") {
    const workflowId = src["getWorkflowId"]();
    context.workflowId = typeof workflowId === "string" ? workflowId : undefined;
  } else if (src["workflowId"] !== undefined) {
    const workflowId = src["workflowId"];
    context.workflowId = typeof workflowId === "string" ? workflowId : undefined;
  }

  // Extract nodeId (method or property)
  if (typeof src["getCurrentNodeId"] === "function") {
    const nodeId = src["getCurrentNodeId"]();
    context.nodeId = typeof nodeId === "string" ? nodeId : undefined;
  } else if (typeof src["getNodeId"] === "function") {
    const nodeId = src["getNodeId"]();
    context.nodeId = typeof nodeId === "string" ? nodeId : undefined;
  } else if (src["nodeId"] !== undefined) {
    const nodeId = src["nodeId"];
    context.nodeId = typeof nodeId === "string" ? nodeId : undefined;
  }

  // Extract AgentLoopEntity-related fields
  const state = src["state"];
  if (state && typeof state === "object" && "currentIteration" in state) {
    const currentIteration = (state as Record<string, unknown>)["currentIteration"];
    if (typeof currentIteration === "number") {
      const id = src["id"];
      context.agentLoopId = typeof id === "string" ? id : undefined;
      context.iteration = currentIteration;
    }
  }

  return context;
}

/**
 * Record error logs (select the level based on severity)
 *
 * @param error SDKError object
 * @param context additional contextual information
 */
export function logError(error: SDKError, context?: Record<string, unknown>): void {
  const logData = {
    errorType: error.constructor.name,
    errorMessage: error.message,
    severity: error.severity,
    ...context,
  };

  switch (error.severity) {
    case "error":
      logger.error(error.message, logData);
      break;
    case "warning":
      logger.warn(error.message, logData);
      break;
    case "info":
      logger.info(error.message, logData);
      break;
  }
}

/**
 * Trigger an error event
 *
 * @param eventManager  Event manager
 * @param params  Event parameters
 */
export async function emitErrorEvent(
  eventManager: EventRegistry | undefined,
  params: {
    threadId: string;
    workflowId: string;
    nodeId?: string;
    error: Error;
  },
): Promise<void> {
  await safeEmit(eventManager, buildErrorEvent(params));
}

/**
 * Unified error handling (logging + event triggering)
 * A convenient function that performs both logging and event triggering simultaneously.
 *
 * @param eventManager  Event manager
 * @param error SDKError object
 * @param params  Event parameters
 */
export async function handleError(
  eventManager: EventRegistry | undefined,
  error: SDKError,
  params: {
    threadId: string;
    workflowId: string;
    nodeId?: string;
  },
): Promise<void> {
  // Log recording
  logError(error, params);

  // Trigger event
  await emitErrorEvent(eventManager, { ...params, error });
}

/**
 * Unified error handling (with automatic context extraction)
 * Automatically extracts context information such as threadId, nodeId, workflowId, etc. from entities
 *
 * @param eventManager  Event manager
 * @param error SDKError object
 * @param contextSource  Context source (WorkflowExecutionEntity, AgentLoopEntity, or a regular object)
 * @param operation  Operation name (optional)
 * @returns  A standardized error containing the extracted context
 */
export async function handleErrorWithContext(
  eventManager: EventRegistry | undefined,
  error: SDKError,
  contextSource: unknown,
  operation?: string,
): Promise<SDKError & { context: StandardErrorContext }> {
  // Extract standard context
  const extractedContext = extractErrorContext(contextSource, operation);

  // Merge the error context from the original source.
  const mergedContext: StandardErrorContext = {
    ...extractedContext,
    ...error.context,
  };

  // Create a new error object with the full context (avoid modifying the original error).
  const enhancedError = Object.create(Object.getPrototypeOf(error));
  Object.assign(enhancedError, error, { context: mergedContext });

  // Log recording and event triggering
  logError(enhancedError, mergedContext);
  await emitErrorEvent(eventManager, {
    threadId: mergedContext.threadId || "",
    workflowId: mergedContext.workflowId || "",
    nodeId: mergedContext.nodeId,
    error: enhancedError,
  });

  return enhancedError;
}
