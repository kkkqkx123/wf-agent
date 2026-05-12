/**
 * Subgraph Node Configuration Type Definition
 * 
 * Configuration for SUBGRAPH nodes that embed workflows as black-box components
 * within the graph structure. These nodes are expanded during preprocessing.
 * 
 * Design Philosophy:
 * - Explicit variable mapping (like function parameters)
 * - No implicit scope inheritance
 * - Clear interface contract between parent and child workflows
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
 * IMPORTANT: All variable passing must be explicit through variableInputs/variableOutputs.
 * There is NO automatic scope inheritance - subgraphs cannot access parent variables
 * unless explicitly mapped.
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
   * Variable Output Mapping
   * 
   * Explicitly defines which subgraph variables are returned to the parent workflow.
   * Only mapped variables will be visible in the parent after subgraph execution.
   * 
   * Example:
   * ```typescript
   * variableOutputs: [
   *   {
   *     internalName: "result",        // Child's output variable
   *     externalName: "processedData", // Parent receives it as this name
   *     description: "Processed result from subgraph"
   *   }
   * ]
   * ```
   */
  variableOutputs?: SubgraphVariableOutput[];
  
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