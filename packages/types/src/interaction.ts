/**
 * User Interaction Type Definition
 * Define the core business types related to user interactions
 */

import type { ID, Metadata } from "./common.js";
import type { VariableScope } from "./workflow-execution/scopes.js";

/**
 * Types of user interactions
 */
export type UserInteractionOperationType =
  /** Updating workflow variables */
  | "UPDATE_VARIABLES"
  /** Adding User Messages to LLM Conversations */
  | "ADD_MESSAGE"
  /** Tool call approval */
  | "TOOL_APPROVAL";

/**
 * Variable Update Configuration
 */
export interface VariableUpdateConfig {
  /** variable name */
  variableName: string;
  /** Variable update expression (may contain {{input}} placeholders) */
  expression: string;
  /** variable scope */
  scope: VariableScope;
}

/**
 * Message Configuration
 */
export interface MessageConfig {
  /** Message role (fixed to 'user') */
  role: "user";
  /** Message content template (may contain {{input}} placeholders) */
  contentTemplate: string;
}

/**
 * User Interaction Requests
 */
export interface UserInteractionRequest {
  /** Interaction ID */
  interactionId: ID;
  /** Type of operation */
  operationType: UserInteractionOperationType;
  /** Variable update configuration (when operationType = UPDATE_VARIABLES) */
  variables?: VariableUpdateConfig[];
  /** Extinguished placement (this operationType = ADD_MESSAGE) */
  message?: MessageConfig;
  /** Prompt message to the user (used by the application layer for display) */
  prompt: string;
  /** Interaction timeout (milliseconds) */
  timeout: number;
  /** Additional operational information */
  metadata?: Metadata;
}

/**
 * User Interaction Response
 */
export interface UserInteractionResponse {
  /** Interaction ID */
  interactionId: ID;
  /** User input data */
  inputData: unknown;
  /** response time stamp */
  timestamp: number;
}

/**
 * User Interaction Processing Results
 */
export interface UserInteractionResult {
  /** Interaction ID */
  interactionId: ID;
  /** Type of operation */
  operationType: UserInteractionOperationType;
  /** Processing results (updated variables or added messages) */
  results: unknown;
  /** Processing timestamps */
  timestamp: number;
}

/**
 * User Interaction Context
 * Execution context provided by the SDK to the application layer
 */
export interface UserInteractionContext {
  /** Thread ID */
  threadId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Node ID */
  nodeId: ID;
  /** Getting the value of a variable */
  getVariable(variableName: string, scope?: VariableScope): unknown;
  /** Setting variable values */
  setVariable(variableName: string, value: unknown, scope?: VariableScope): Promise<void>;
  /** Get all variables */
  getVariables(scope?: VariableScope): Record<string, unknown>;
  /** timeout control */
  timeout: number;
  /** Cancel Token */
  cancelToken: {
    cancelled: boolean;
    cancel(): void;
  };
}

/**
 * Tool Approval Data Structure
 * For tool approval requests and responses
 */
export interface ToolApprovalData {
  /** Tool ID */
  toolId: ID;
  /** Tool name */
  toolName?: string;
  /** Tool Description */
  toolDescription: string;
  /** Tool parameters */
  toolParameters: Record<string, unknown>;
  /** Approval or not */
  approved: boolean;
  /** Edited parameters (optional) */
  editedParameters?: Record<string, unknown>;
  /** User commands (optional) */
  userInstruction?: string;
}

/**
 * User Interaction Processor Interface
 * An interface that must be implemented by the application layer to obtain user input.
 */
export interface UserInteractionHandler {
  /**
   * Handling User Interaction Requests
   * @param request Interaction request
   * @param context Interaction context
   * @returns User input data
   */
  handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown>;
}
