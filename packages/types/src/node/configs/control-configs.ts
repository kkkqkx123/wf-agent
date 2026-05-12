/**
 * Control Node Configuration Type Definition
 * Contains START, END, and ROUTE node configurations
 * 
 * Design Philosophy:
 * - START node defines explicit variable inputs (like function parameters)
 * - START is the entry point only, should not have outputs
 * - Especially important for subgraphs where START receives mapped variables from parent
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
 * Starting Node Configuration
 * 
 * Extended to support explicit declaration of message context AND variable inputs,
 * especially for subgraphs. This provides a clear interface contract similar to function parameters.
 * 
 * IMPORTANT: For subgraphs, ALL input variables must be explicitly declared here.
 * Variables are passed from parent workflow through SUBGRAPH node's variableInputs mapping.
 * START node acts as the entry point only - it does NOT define outputs.
 * Outputs are handled by END nodes or returned via the execution result.
 */
export interface StartNodeConfig {
  /**
   * Variable Inputs
   * 
   * Explicitly defines which parent workflow variables are accessible within this workflow.
   * This is the ONLY way for a subgraph to receive variables from its parent.
   * 
   * For root workflows, this field is typically empty.
   * For subgraphs, these inputs are mapped from SUBGRAPH node's variableInputs configuration.
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