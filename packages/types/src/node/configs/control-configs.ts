/**
 * Control Node Configuration Type Definition
 * Contains START, END, and ROUTE node configurations
 * 
 * Design Philosophy:
 * - START node defines explicit variable inputs/outputs (like function signature)
 * - Especially important for subgraphs where START is the entry point
 */

import type { Condition } from "../../graph/condition.js";

/**
 * Variable Input Mapping for START node
 * Defines which parent workflow variables are accessible within this workflow
 */
export interface StartVariableInput {
  /** Parent workflow variable name (source) */
  externalName: string;
  
  /** Internal variable name within this workflow */
  internalName: string;
  
  /** Whether this input is required */
  required?: boolean;
  
  /** Default value if parent variable is not found */
  defaultValue?: unknown;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Variable Output Mapping for END node
 * Defines which workflow variables are returned to the caller
 */
export interface StartVariableOutput {
  /** Internal variable name within this workflow */
  internalName: string;
  
  /** Name visible to the caller (parent workflow) */
  externalName: string;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Starting Node Configuration
 * 
 * Extended to support explicit declaration of message context AND variable inputs/outputs,
 * especially for subgraphs. This provides a clear interface contract similar to function signatures.
 * 
 * IMPORTANT: For subgraphs, ALL variables must be explicitly declared here.
 * There is NO automatic scope inheritance from parent workflow.
 */
export interface StartNodeConfig {
  /**
   * Variable Inputs
   * 
   * Explicitly defines which parent workflow variables are accessible within this workflow.
   * This is the ONLY way for a subgraph to receive variables from its parent.
   * 
   * Example:
   * ```typescript
   * variableInputs: [
   *   {
   *     externalName: "apiKey",        // Parent's variable
   *     internalName: "api_key",       // This workflow sees it as this name
   *     required: true,
   *     description: "API key for authentication"
   *   },
   *   {
   *     externalName: "config",
   *     internalName: "settings",
   *     defaultValue: { timeout: 5000 }
   *   }
   * ]
   * ```
   */
  variableInputs?: StartVariableInput[];
  
  /**
   * Variable Outputs
   * 
   * Explicitly defines which workflow variables are returned to the caller.
   * Only mapped variables will be visible in the parent after workflow execution.
   * 
   * Example:
   * ```typescript
   * variableOutputs: [
   *   {
   *     internalName: "result",        // This workflow's output variable
   *     externalName: "processedData", // Parent receives it as this name
   *     description: "Processed result"
   *   }
   * ]
   * ```
   */
  variableOutputs?: StartVariableOutput[];
  
  /**
   * Message context inputs
   * 
   * Defines the message contexts that this workflow (especially subgraphs)
   * expects to receive from the caller.
   */
  messageInputs?: Array<{
    /** Name used by the caller (parent workflow) */
    externalName: string;
    
    /** Name used internally within this workflow */
    internalName: string;
    
    /** Whether this input is required */
    required?: boolean;
    
    /** Description for documentation */
    description?: string;
  }>;
  
  /**
   * Message context outputs
   * 
   * Defines the message contexts that this workflow produces for the caller.
   */
  messageOutputs?: Array<{
    /** Name used internally within this workflow */
    internalName: string;
    
    /** Name visible to the caller (parent workflow) */
    externalName: string;
    
    /** Description for documentation */
    description?: string;
  }>;
}

/**
 * End Node Configuration
 * No configuration, only as workflow end flag
 */
export type EndNodeConfig = object;

/**
 * Routing Node Configuration
 */
export interface RouteNodeConfig {
  /** Routing Rules Array */
  routes: Array<{
    /** conditional expression */
    condition: Condition;
    /** Target Node ID */
    targetNodeId: string;
    /** prioritization */
    priority?: number;
  }>;
  /** Default target node ID */
  defaultTargetNodeId?: string;
}