/**
 * Workflow Boundary Data Passing Configuration
 *
 * This module defines unified configuration interfaces for data passing at workflow boundaries.
 * All entry and exit nodes (START, END, SUBGRAPH_START, SUBGRAPH_END, START_FROM_TRIGGER, CONTINUE_FROM_TRIGGER)
 * should use these configurations to ensure consistency across different execution scenarios.
 *
 * Design Philosophy:
 * - Workflows are treated as functions with explicit input/output contracts
 * - Start nodes declare input parameters (like function signatures)
 * - End nodes declare output return values (like return statements)
 * - Clear separation between variables (structured data) and messages (LLM conversation history)
 */

import type { LLMMessage } from "../message/index.js";

// ============================================================================
// Variable Input/Output Configuration
// ============================================================================

/**
 * Variable Input Definition
 * Defines how external variables are mapped into the workflow's internal scope
 */
export interface WorkflowVariableInput {
  /** External variable name (used by caller/parent workflow) */
  externalName: string;

  /** Internal variable name (used within this workflow) */
  internalName: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default value if external variable is not provided */
  defaultValue?: unknown;

  /** Description for documentation and tool hints */
  description?: string;
}

/**
 * Variable Output Definition
 * Defines how internal variables are returned to the caller/parent workflow
 */
export interface WorkflowVariableOutput {
  /** Internal variable name (source within this workflow) */
  internalName: string;

  /** External variable name (target for caller/parent workflow) */
  externalName: string;

  /** Description for documentation */
  description?: string;
}

// ============================================================================
// Message Context Input/Output Configuration
// ============================================================================

/**
 * Message Context Input Definition
 * Defines how named message contexts are passed into the workflow
 */
export interface WorkflowMessageInput {
  /** External context ID (used by caller/parent workflow) */
  externalName: string;

  /** Internal context ID (used within this workflow) */
  internalName: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default messages if external context is not provided */
  defaultMessages?: LLMMessage[];

  /** Description for documentation */
  description?: string;
}

/**
 * Message Context Output Definition
 * Defines how named message contexts are returned from the workflow
 */
export interface WorkflowMessageOutput {
  /** Internal context ID (source within this workflow) */
  internalName: string;

  /** External context ID (target for caller/parent workflow) */
  externalName: string;

  /** Description for documentation */
  description?: string;
}

// ============================================================================
// Data Input/Output Configuration
// ============================================================================

/**
 * Data Input Definition
 *
 * Maps a field from the caller's WorkflowExecution.input to an internal variable.
 * This enables explicit data passing from the execution-level input (set when a workflow
 * is triggered or called) into the workflow's variable system.
 *
 * Unlike WorkflowVariableInput (which maps parent variables to child variables),
 * WorkflowDataInput maps the raw input data of the execution to internal variables.
 * This is the mechanism for passing top-level execution data across boundary.
 *
 * Example:
 * When a workflow is called with { userId: "abc", query: "hello" },
 * dataInputs can map:
 *   { parentField: "userId", internalName: "user_id" } → variable "user_id" gets "abc"
 *   { parentField: "query", internalName: "query_text" } → variable "query_text" gets "hello"
 */
export interface WorkflowDataInput {
  /** Key in the caller's WorkflowExecution.input */
  parentField: string;

  /** Internal variable name to store this value */
  internalName: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default value if parent doesn't provide this field */
  defaultValue?: unknown;

  /** Description for documentation */
  description?: string;
}

/**
 * Data Output Definition
 *
 * Maps an internal variable to a key in the workflow's output data.
 * This enables explicit data passing from the workflow's variable system
 * back to the execution-level output.
 *
 * Unlike WorkflowVariableOutput (which maps child variables to parent variables),
 * WorkflowDataOutput maps internal variables to the execution output data.
 * This is the mechanism for returning data across boundary.
 *
 * Example:
 * When a workflow has variables { result: "...", error: null },
 * dataOutputs can map:
 *   { internalName: "result", outputKey: "data" } → output.data gets the result
 *   { internalName: "error", outputKey: "error" } → output.error gets null
 */
export interface WorkflowDataOutput {
  /** Internal variable name to export */
  internalName: string;

  /** Key in the workflow's output data */
  outputKey: string;

  /** Description for documentation */
  description?: string;
}

// ============================================================================
// Unified Boundary Configuration Interfaces
// ============================================================================

/**
 * Unified Start Node Configuration
 * Used by: START, SUBGRAPH_START, START_FROM_TRIGGER
 *
 * Declares the input contract for a workflow, specifying which variables,
 * message contexts, and data inputs it expects to receive from the caller.
 *
 * Data inputs enable mapping from the caller's execution input data to
 * internal variables, creating an explicit data passing contract.
 */
export interface WorkflowStartConfig {
  /**
   * Variable inputs - maps external variables to internal scope
   */
  variableInputs?: WorkflowVariableInput[];

  /**
   * Message context inputs - maps external message contexts to internal contexts
   */
  messageInputs?: WorkflowMessageInput[];

  /**
   * Data inputs - declares which keys this workflow expects in its execution input
   * and maps them to internal variables.
   *
   * This creates an explicit contract: the caller MUST provide these fields
   * in the execution input (or have defaults), and they will be available
   * as variables within this workflow.
   */
  dataInputs?: WorkflowDataInput[];
}

/**
 * Unified End Node Configuration
 * Used by: END, SUBGRAPH_END, CONTINUE_FROM_TRIGGER
 *
 * Declares the output contract for a workflow, specifying which variables,
 * message contexts, and data outputs it returns to the caller.
 *
 * Data outputs enable mapping from internal variables to the execution output,
 * creating an explicit return value contract.
 */
export interface WorkflowEndConfig {
  /**
   * Variable outputs - maps internal variables to external return values
   */
  variableOutputs?: WorkflowVariableOutput[];

  /**
   * Message context outputs - maps internal contexts to external contexts
   */
  messageOutputs?: WorkflowMessageOutput[];

  /**
   * Data outputs - declares which variables to export to the execution output.
   *
   * This creates an explicit contract: when this workflow completes, these
   * variables will be available in the execution output under the specified keys.
   */
  dataOutputs?: WorkflowDataOutput[];
}

/**
 * Complete boundary configuration combining start and end
 * Useful for documenting workflow interfaces or generating schemas
 */
export interface WorkflowBoundaryConfig {
  /** Start node configuration (inputs) */
  start: WorkflowStartConfig;

  /** End node configuration (outputs) */
  end: WorkflowEndConfig;
}

// ============================================================================
// Boundary Node Output Interfaces
// ============================================================================

/**
 * START / EMBED_START / START_FROM_TRIGGER node output shape.
 * Matches the actual return value of start-handler.ts.
 */
export interface StartNodeOutput {
  message: string;
  input?: unknown;
}

/**
 * END / EMBED_END / CONTINUE_FROM_TRIGGER node output shape.
 * Matches the actual return value of end-handler.ts.
 */
export interface EndNodeOutput {
  output: unknown;
}
