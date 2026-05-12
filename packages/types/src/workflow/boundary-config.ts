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

import type { LLMMessage } from '../message/index.js';

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
// Unified Boundary Configuration Interfaces
// ============================================================================

/**
 * Unified Start Node Configuration
 * Used by: START, SUBGRAPH_START, START_FROM_TRIGGER
 * 
 * Declares the input contract for a workflow, specifying which variables
 * and message contexts it expects to receive from the caller.
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
}

/**
 * Unified End Node Configuration
 * Used by: END, SUBGRAPH_END, CONTINUE_FROM_TRIGGER
 * 
 * Declares the output contract for a workflow, specifying which variables
 * and message contexts it returns to the caller.
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
