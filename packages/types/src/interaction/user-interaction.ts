/**
 * User Interaction Core Types
 * Define the core business types related to user interactions
 * 
 * This module defines the general-purpose interaction protocol for app-level UI interactions.
 * Examples: tool approval (requires confirmation UI), follow-up questions (requires options UI).
 * 
 * Note: This is separate from workflow node configuration. Workflow-specific operations
 * like UPDATE_VARIABLES and ADD_MESSAGE are defined in packages/types/src/node/configs/interaction-configs.ts.
 */

import type { ID, Metadata } from "../common.js";

/**
 * Types of user interactions (App-level UI interactions)
 */
export type UserInteractionOperationType =
  /** Tool call approval (requires confirmation UI) */
  | "TOOL_APPROVAL"
  /** Ask follow-up question (requires options UI) */
  | "ASK_FOLLOWUP_QUESTION"
  /** Script interactive execution (requires user input for interactive scripts) */
  | "SCRIPT_INTERACTION";

/**
 * User Interaction Requests
 * 
 * General-purpose interaction request for app-level UI interactions.
 * For workflow-specific operations (UPDATE_VARIABLES, ADD_MESSAGE), 
 * use UserInteractionNodeConfig instead.
 */
export interface UserInteractionRequest {
  /** Interaction ID */
  interactionId: ID;
  /** Type of operation */
  operationType: UserInteractionOperationType;
  /** Prompt message to the user (used by the application layer for display) */
  prompt: string;
  /** Interaction timeout (milliseconds) */
  timeout: number;
  /** Additional operational information */
  metadata?: Metadata & {
    // Structured tool approval data
    toolData?: import("./tool-approval.js").ToolApprovalRequestData;
    // Structured follow-up question data
    followupData?: import("./followup-question.js").FollowupQuestionRequestData;
  };
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
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Node ID */
  nodeId: ID;
  /** Getting the value of a variable */
  getVariable(variableName: string): unknown;
  /** Setting variable values */
  setVariable(variableName: string, value: unknown): Promise<void>;
  /** Get all variables */
  getVariables(): Record<string, unknown>;
  /** timeout control */
  timeout: number;
  /** Cancel Token */
  cancelToken: {
    cancelled: boolean;
    cancel(): void;
  };
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
