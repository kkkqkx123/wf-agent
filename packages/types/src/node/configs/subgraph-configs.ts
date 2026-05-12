/**
 * Subgraph Node Configuration Type Definition
 * 
 * Configuration for SUBGRAPH nodes that embed workflows as black-box components
 * within the graph structure. These nodes are expanded during preprocessing.
 * 
 * Design Philosophy:
 * - Explicit variable mapping (like function parameters)
 * - Variable isolation through scope stack mechanism
 * - Clear interface contract between parent and child workflows
 * - START node in subgraph receives mapped inputs, but does NOT define outputs
 */

import type { ID } from '../../common.js';

/**
 * Variable Input Mapping
 * Defines how parent workflow variables are passed to subgraph
 */
export interface SubgraphVariableInput {
  /** Parent workflow variable name (source) */
  externalName: string;
  
  /** Subgraph internal variable name (target) */
  internalName: string;
  
  /** Whether this input is required */
  required?: boolean;
  
  /** Default value if parent variable is not found */
  defaultValue?: unknown;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Variable Output Mapping
 * Defines how subgraph variables are returned to parent workflow
 */
export interface SubgraphVariableOutput {
  /** Subgraph internal variable name (source) */
  internalName: string;
  
  /** Parent workflow variable name (target) */
  externalName: string;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Subgraph node configuration
 * 
 * Used for embedding workflows as black-box nodes within the graph structure.
 * The subgraph is expanded and merged into the parent graph during preprocessing.
 * 
 * IMPORTANT: All variable passing must be explicit through variableInputs.
 * There is NO automatic scope inheritance - subgraphs cannot access parent variables
 * unless explicitly mapped. Variable isolation is achieved through VariableManager's
 * scope stack mechanism.
 * 
 * Note: variableOutputs has been removed from this configuration. Output values
 * should be handled through the workflow's execution result or END node configuration.
 */
export interface SubgraphNodeConfig {
  /** Subworkflow ID to embed */
  subgraphId: ID;
  
  /** Whether to execute asynchronously */
  async: boolean;
  
  /**
   * Variable Input Mapping
   * 
   * Explicitly defines which parent workflow variables are passed to the subgraph.
   * This is the ONLY way for a subgraph to receive data from its parent.
   * 
   * During graph preprocessing, these mappings are transferred to the subgraph's
   * START node configuration, making them available at runtime.
   * 
   * Example:
   * ```typescript
   * variableInputs: [
   *   {
   *     externalName: "apiKey",        // Parent's variable
   *     internalName: "api_key",       // Child sees it as this name
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
  variableInputs?: SubgraphVariableInput[];
  
  /**
   * Message context passing configuration
   * 
   * Maps parent workflow contexts to subgraph inputs,
   * and subgraph outputs back to parent workflow contexts.
   * This provides explicit control over message context flow between workflows.
   */
  messagePassing?: {
    /** Input mapping: parentContextId → subgraphInputExternalName */
    inputs?: Record<string, string>;
    
    /** Output mapping: subgraphOutputExternalName → parentContextId */
    outputs?: Record<string, string>;
  };
}