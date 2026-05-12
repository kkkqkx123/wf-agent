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
 * - Reuses WorkflowBoundaryConfig for consistent boundary handling across all workflow types
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableInput, WorkflowVariableOutput } from '../../workflow/boundary-config.js';



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
 * Note: This configuration reuses WorkflowVariableInput/WorkflowVariableOutput from
 * boundary-config.ts to maintain consistency with other workflow boundary configurations
 * (START, END, START_FROM_TRIGGER, CONTINUE_FROM_TRIGGER).
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
   * Uses WorkflowVariableInput for consistency with other boundary configurations.
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
  variableInputs?: WorkflowVariableInput[];
  
  /**
   * Variable Output Mapping
   * 
   * Defines how subgraph variables are returned to the parent workflow.
   * Uses WorkflowVariableOutput for consistency with other boundary configurations.
   * 
   * Note: While this field exists in the SUBGRAPH node config for documentation
   * and validation purposes, actual output handling is done through the subgraph's
   * END node configuration or execution result.
   */
  variableOutputs?: WorkflowVariableOutput[];
  
  /**
   * Message context passing configuration
   * 
   * Maps parent workflow contexts to subgraph inputs,
   * and subgraph outputs back to parent workflow contexts.
   * This provides explicit control over message context flow between workflows.
   * 
   * Note: This is different from WorkflowMessageInput/WorkflowMessageOutput used
   * in START/END nodes. Those define the boundary contract of a workflow itself,
   * while this defines how the SUBGRAPH node maps contexts between parent and child.
   */
  messagePassing?: {
    /** Input mapping: parentContextId → subgraphInputExternalName */
    inputs?: Record<string, string>;
    
    /** Output mapping: subgraphOutputExternalName → parentContextId */
    outputs?: Record<string, string>;
  };
}