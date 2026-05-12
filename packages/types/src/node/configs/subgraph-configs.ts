/**
 * Subgraph Node Configuration Type Definition
 * 
 * Configuration for SUBGRAPH nodes that embed workflows as black-box components
 * within the graph structure. These nodes are expanded during preprocessing.
 */

import type { ID } from '../../common.js';

/**
 * Subgraph node configuration
 * 
 * Used for embedding workflows as black-box nodes within the graph structure.
 * The subgraph is expanded and merged into the parent graph during preprocessing.
 */
export interface SubgraphNodeConfig {
  /** Subworkflow ID to embed */
  subgraphId: ID;
  
  /** Whether to execute asynchronously */
  async: boolean;
  
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