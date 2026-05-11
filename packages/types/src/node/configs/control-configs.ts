/**
 * Control Node Configuration Type Definition
 * Contains START, END, and ROUTE node configurations
 */

import type { Condition } from "../../graph/condition.js";

/**
 * Starting Node Configuration
 * 
 * Extended to support explicit declaration of message context inputs and outputs,
 * especially for subgraphs. This provides a clear interface contract similar to function signatures.
 */
export interface StartNodeConfig {
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