/**
 * User Interaction Node Processor
 * Responsible for executing the USER_INTERACTION node, handling user input, and updating variables/adding messages.
 *
 * Design Principles:
 * - Only contains the core execution logic; does not include event triggering.
 * - Receives verified configuration.
 * - Returns the execution results.
 *
 * Note: This handler is workflow-specific and handles UPDATE_VARIABLES and ADD_MESSAGE operations.
 * It does NOT use the general UserInteractionRequest protocol (which is for app-level UI interactions
 * like tool approval and follow-up questions).
 */

import type { RuntimeNode, UserInteractionNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type {
  UserInteractionHandler as AppUserInteractionHandler,
  UserInteractionContext,
} from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { generateId } from "../../../../utils/index.js";

/**
 * Minimal interface for VariableManager access
 * Avoids circular dependency with WorkflowExecutionEntity
 */
interface VariableManagerAccess {
  getVariable: (name: string) => unknown;
  setVariable: (name: string, value: unknown) => void;
  getAllVariables: () => Record<string, unknown>;
}

/**
 * Workflow-specific interaction request
 * Internal type for passing workflow node config to the interaction handler
 */
interface WorkflowInteractionRequest {
  interactionId: string;
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  variables?: Array<{
    variableName: string;
    expression: string;
  }>;
  message?: {
    role: 'user';
    contentTemplate: string;
  };
  prompt: string;
  timeout: number;
  metadata?: Record<string, unknown>;
}

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
 * Create a workflow-specific interaction request from node config
 */
function createWorkflowInteractionRequest(
  config: UserInteractionNodeConfig,
  interactionId: string,
): WorkflowInteractionRequest {
  return {
    interactionId,
    operationType: config.operationType,
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
  node: RuntimeNode,
  timeout: number,
  _conversationManager?: unknown,
  variableManager?: VariableManagerAccess,
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
    getVariable: (variableName: string) => {
      if (variableManager) {
        return variableManager.getVariable(variableName);
      }
      return undefined;
    },
    setVariable: async (variableName: string, value: unknown) => {
      if (variableManager) {
        variableManager.setVariable(variableName, value);
      }
    },
    getVariables: () => {
      if (variableManager) {
        return variableManager.getAllVariables();
      }
      return {};
    },
    timeout,
    cancelToken,
  };
}

/**
 * Get user input
 */
async function getUserInput(
  request: WorkflowInteractionRequest,
  context: UserInteractionContext,
  handler: AppUserInteractionHandler,
): Promise<unknown> {
  // Implementing timeout control
  const timeoutPromise = new Promise((_, reject) => {
    setTimeout(() => {
      reject(new Error(`User interaction timeout after ${request.timeout}ms`));
    }, request.timeout);
  });

  // Cancel control with proper cleanup
  let cancelInterval: NodeJS.Timeout | null = null;
  const cancelPromise = new Promise((_, reject) => {
    cancelInterval = setInterval(() => {
      if (context.cancelToken.cancelled) {
        if (cancelInterval) {
          clearInterval(cancelInterval);
          cancelInterval = null;
        }
        reject(new Error("User interaction cancelled"));
      }
    }, 100);
  });

  try {
    // Convert WorkflowInteractionRequest to UserInteractionRequest for compatibility
    // Map workflow operation types to app-level operation types
    const mappedOperationType: import("@wf-agent/types").UserInteractionOperationType = 
      request.operationType === 'UPDATE_VARIABLES' ? 'TOOL_APPROVAL' : 'ASK_FOLLOWUP_QUESTION';
    
    const appRequest: import("@wf-agent/types").UserInteractionRequest = {
      interactionId: request.interactionId,
      operationType: mappedOperationType,
      prompt: request.prompt,
      timeout: request.timeout,
      metadata: request.metadata,
    };
    
    // Competition: User input, timeouts, cancellations
    return await Promise.race([handler.handle(appRequest, context), timeoutPromise, cancelPromise]);
  } finally {
    // Properly clean up interval to prevent memory leaks
    if (cancelInterval) {
      clearInterval(cancelInterval);
      cancelInterval = null;
    }
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
  variableManager?: VariableManagerAccess,
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

    // Update the variable using VariableManager API
    if (variableManager) {
      variableManager.setVariable(variableConfig.variableName, value);
    }

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
  variableManager?: VariableManagerAccess,
): Promise<unknown> {
  switch (config.operationType) {
    case "UPDATE_VARIABLES":
      return await processVariableUpdate(config, inputData, workflowExecution, variableManager);

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
 * @param executionEntity Workflow execution entity for variable access
 * @returns Execution result
 */
export async function userInteractionHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: UserInteractionHandlerContext,
  executionEntity?: { variableStateManager: VariableManagerAccess },
): Promise<import("@wf-agent/types").UserInteractionNodeOutput> {
  const config = node.config as UserInteractionNodeConfig;
  const interactionId = generateId();

  // 1. Create a workflow-specific interaction request
  const request = createWorkflowInteractionRequest(config, interactionId);

  // 2. Create an interactive context
  const interactionContext = createInteractionContext(
    workflowExecution,
    node,
    request.timeout,
    context.conversationManager,
    executionEntity?.variableStateManager,
  );

  // 3. Obtain user input
  const inputData = await getUserInput(
    request,
    interactionContext as UserInteractionContext,
    context.userInteractionHandler,
  );

  // 4. Processing user input
  const results = await processUserInput(
    config,
    inputData,
    workflowExecution,
    context.conversationManager,
    executionEntity?.variableStateManager,
  );

  // Build output matching UserInteractionNodeOutput
  const output: import("@wf-agent/types").UserInteractionNodeOutput = {
    operationType: config.operationType,
    userInput: inputData,
  };

  if (config.operationType === 'UPDATE_VARIABLES' && typeof results === 'object' && results !== null) {
    output.updatedVariables = Object.entries(results).map(([variableName, newValue]) => ({
      variableName,
      newValue,
    }));
  } else if (config.operationType === 'ADD_MESSAGE') {
    output.addedMessages = 1;
  }

  return output;
}
