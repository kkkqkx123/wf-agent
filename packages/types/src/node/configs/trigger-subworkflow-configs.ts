/**
 * Trigger-based Subworkflow Node Configuration Type Definitions
 * 
 * These configurations are for nodes used in trigger-initiated subworkflows,
 * which exist outside the main graph structure and are executed asynchronously via triggers.
 * 
 * Key differences from SUBGRAPH nodes:
 * - SUBGRAPH: Embedded as black-box nodes within the graph, expanded during preprocessing
 * - Trigger Subworkflow: Completely independent execution, initiated by trigger actions
 */

/**
 * Start From Trigger Node Configuration
 * 
 * Serves as the entry point identifier for isolated subworkflows initiated by triggers.
 * Similar to START node but specifically for trigger-based execution.
 * 
 * Extended to support explicit declaration of message context inputs,
 * providing a clear interface contract for data passed from the triggering workflow.
 */
export interface StartFromTriggerNodeConfig {
  /**
   * Message context inputs
   * 
   * Defines the message contexts that this trigger subworkflow expects to receive
   * from the triggering workflow or action.
   */
  messageInputs?: Array<{
    /** Name used by the caller (triggering workflow/action) */
    externalName: string;
    
    /** Name used internally within this subworkflow */
    internalName: string;
    
    /** Whether this input is required */
    required?: boolean;
    
    /** Description for documentation */
    description?: string;
  }>;
}

/**
 * Continue From Trigger Node Configuration
 * 
 * Serves as the exit point for trigger subworkflows and handles data callback
 * to the parent/main workflow after execution completes.
 * 
 * Similar to END node but with additional callback configuration for returning
 * variables back to the triggering context.
 * 
 * Note: Message context filtering/truncation has been migrated to the unified
 * reference architecture and should be handled separately.
 */
export interface ContinueFromTriggerNodeConfig {
  /**
   * Message context outputs
   * 
   * Defines the message contexts that this trigger subworkflow produces
   * for the calling workflow.
   */
  messageOutputs?: Array<{
    /** Name used internally within this subworkflow */
    internalName: string;
    
    /** Name visible to the caller (parent workflow) */
    externalName: string;
    
    /** Description for documentation */
    description?: string;
  }>;

  /**
   * Variable Callback Configuration
   * 
   * Controls which workflow variables are returned to the parent workflow
   * after the trigger subworkflow completes.
   */
  variableCallback?: {
    /** List of variable names to return */
    includeVariables?: string[];
    
    /** Whether to pass back all variables (default false) */
    includeAll?: boolean;
  };
}
