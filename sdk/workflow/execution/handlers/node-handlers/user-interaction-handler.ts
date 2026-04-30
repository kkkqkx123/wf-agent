/**
 * User Interaction Node Processor
 * Responsible for executing the USER_INTERACTION node, handling user input, and updating variables/adding messages.
 *
 * Design Principles:
 * - Only contains the core execution logic; does not include event triggering.
 * - Receives verified configuration.
 * - Returns the execution results.
 */

import type { Node, UserInteractionNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type {
  UserInteractionHandler as AppUserInteractionHandler,
  UserInteractionRequest,
  UserInteractionContext,
} from "@wf-agent/types";
import type { VariableScope } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { generateId } from "../../../../utils/id-utils.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * User Interaction Execution Context
 */
export interface UserInteractionHandlerContext {
  /** User Interaction Processor */
  userInteractionHandler: AppUserInteractionHandler;
  /** Dialogue Manager */
  conversationManager?: {
    addMessage: (message: { role: string; content: string }) => void;
  };
  /** Timeout period */
  timeout?: number;
}

/**
 * User interaction execution results
 */
export interface UserInteractionExecutionResult {
  /** Interaction ID */
  interactionId: string;
  /** Operation Type */
  operationType: string;
  /** Processing results */
  results: unknown;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Create an interactive request
 */
function createInteractionRequest(
  config: UserInteractionNodeConfig,
  interactionId: string,
): UserInteractionRequest {
  return {
    interactionId,
    operationType: config.operationType as unknown as UserInteractionRequest["operationType"],
    variables: config.variables,
    message: config.message,
    prompt: config.prompt,
    timeout: config.timeout || 30000,
    metadata: config.metadata,
  };
}

/**
 * Create an interactive context
 */
function createInteractionContext(
  workflowExecution: WorkflowExecution,
  node: Node,
  timeout: number,
  _conversationManager?: unknown,
): unknown {
  const cancelToken = {
    cancelled: false,
    cancel: () => {
      cancelToken.cancelled = true;
    },
  };

  return {
    executionId: workflowExecution.id,
    workflowId: workflowExecution.workflowId,
    nodeId: node.id,
    getVariable: (variableName: string, _scope?: VariableScope) => {
      // Simplify the implementation; in reality, the variable should be retrieved from the workflow execution.
      return workflowExecution.variableScopes.workflowExecution?.[variableName];
    },
    setVariable: async (variableName: string, value: unknown, _scope?: VariableScope) => {
      // Simplify the implementation; in reality, the variable in the workflow execution should be updated.
      if (!workflowExecution.variableScopes.workflowExecution) {
        workflowExecution.variableScopes.workflowExecution = {};
      }
      workflowExecution.variableScopes.workflowExecution[variableName] = value;
    },
    getVariables: (_scope?: VariableScope) => {
      return workflowExecution.variableScopes.workflowExecution || {};
    },
    timeout,
    cancelToken,
  };
}

/**
 * Get user input
 */
async function getUserInput(
  request: UserInteractionRequest,
  context: UserInteractionContext,
  handler: AppUserInteractionHandler,
): Promise<unknown> {
  // Implementing timeout control
  const timeoutPromise = new Promise((_, reject) => {
    setTimeout(() => {
      reject(new Error(`User interaction timeout after ${request.timeout}ms`));
    }, request.timeout);
  });

  // Cancel control
  const cancelPromise = new Promise((_, reject) => {
    const checkCancel = setInterval(() => {
      if (context.cancelToken.cancelled) {
        clearInterval(checkCancel);
        reject(new Error("User interaction cancelled"));
      }
    }, 100);
  });

  try {
    // Competition: User input, timeouts, cancellations
    return await Promise.race([handler.handle(request, context), timeoutPromise, cancelPromise]);
  } finally {
    // Clean up the cancellation check.
    context.cancelToken.cancel();
  }
}

/**
 * Replace the {{input}} placeholder.
 */
function replaceInputPlaceholder(template: string, inputData: unknown): string {
  if (typeof template !== "string") {
    return String(template);
  }

  // Replace the {{input}} placeholder.
  return template.replace(/\{\{input\}\}/g, String(inputData));
}

/**
 * Calculate the value of the expression (simple implementation)
 */
function evaluateExpression(expression: string, inputData: unknown): unknown {
  // If the expression is just {{input}}, return the input directly.
  if (expression === "{{input}}") {
    return inputData;
  }

  // If the expression contains {{input}}, return the result after replacement.
  if (expression.includes("{{input}}")) {
    return replaceInputPlaceholder(expression, inputData);
  }

  // Otherwise, simply return the expression (which may be a constant value).
  return expression;
}

/**
 * Handling variable updates
 */
async function processVariableUpdate(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  workflowExecution: WorkflowExecution,
): Promise<Record<string, unknown>> {
  if (!config.variables || config.variables.length === 0) {
    throw new ExecutionError("No variables defined for UPDATE_VARIABLES operation", workflowExecution.id);
  }

  const results: Record<string, unknown> = {};

  for (const variableConfig of config.variables) {
    // Replace the {{input}} placeholder in the expression.
    const expression = replaceInputPlaceholder(variableConfig.expression, inputData);

    // Calculate the value of the expression.
    const value = evaluateExpression(expression, inputData);

    // Update the variable
    if (!workflowExecution.variableScopes.workflowExecution) {
      workflowExecution.variableScopes.workflowExecution = {};
    }
    workflowExecution.variableScopes.workflowExecution[variableConfig.variableName] = value;

    results[variableConfig.variableName] = value;
  }

  return results;
}

/**
 * Handle message addition
 */
function processMessageAdd(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  conversationManager?: UserInteractionHandlerContext["conversationManager"],
): { role: string; content: string } {
  if (!config.message) {
    throw new ExecutionError("No message defined for ADD_MESSAGE operation");
  }

  // Replace the {{input}} placeholder in the content template.
  const content = replaceInputPlaceholder(config.message.contentTemplate, inputData);

  // Add messages to the conversation manager.
  if (conversationManager) {
    conversationManager.addMessage({
      role: config.message.role,
      content,
    });
  }

  return {
    role: config.message.role,
    content,
  };
}

/**
 * Processing user input
 */
async function processUserInput(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  workflowExecution: WorkflowExecution,
  conversationManager?: UserInteractionHandlerContext["conversationManager"],
): Promise<unknown> {
  switch (config.operationType) {
    case "UPDATE_VARIABLES":
      return await processVariableUpdate(config, inputData, workflowExecution);

    case "ADD_MESSAGE":
      return processMessageAdd(config, inputData, conversationManager);

    default:
      throw new ExecutionError(`Unknown operation type: ${config.operationType}`);
  }
}

/**
 * User Interaction Node Processor
 * @param workflowExecution Workflow execution instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function userInteractionHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: UserInteractionHandlerContext,
): Promise<UserInteractionExecutionResult> {
  const config = node.config as UserInteractionNodeConfig;
  const interactionId = generateId();
  const startTime = now();

  // 1. Create an interactive request
  const request = createInteractionRequest(config, interactionId);

  // 2. Create an interactive context
  const interactionContext = createInteractionContext(
    workflowExecution,
    node,
    request.timeout,
    context.conversationManager,
  );

  // 3. Obtain user input
  const inputData = await getUserInput(
    request,
    interactionContext as UserInteractionContext,
    context.userInteractionHandler,
  );

  // 4. Processing user input
  const results = await processUserInput(config, inputData, workflowExecution, context.conversationManager);

  const executionTime = diffTimestamp(startTime, now());

  return {
    interactionId,
    operationType: config.operationType,
    results,
    executionTime,
  };
}
