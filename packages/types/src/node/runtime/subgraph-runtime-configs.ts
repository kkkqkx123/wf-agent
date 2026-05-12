/**
 * Subgraph Runtime Node Configuration Type Definitions
 * 
 * These configurations are used ONLY at runtime after graph preprocessing.
 * They should NOT appear in static workflow TOML definitions.
 * 
 * Design Philosophy:
 * - SUBGRAPH_START replaces START nodes in expanded subgraphs
 * - SUBGRAPH_END replaces END nodes in expanded subgraphs
 * - These nodes handle variable scope management and data mapping
 * - Clear separation between static config (user-defined) and runtime config (system-generated)
 */

import type { ID } from '../../common.js';

// ============================================================================
// SUBGRAPH_START Node Configuration
// ============================================================================

/**
 * Variable Input Mapping for SUBGRAPH_START node
 * 
 * Defines how parent workflow variables are mapped to subgraph internal variables.
 * This configuration is transferred from the parent SUBGRAPH node during preprocessing.
 */
export interface SubgraphStartVariableInput {
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
 * Message Context Input Mapping for SUBGRAPH_START node
 */
export interface SubgraphStartMessageInput {
  /** Parent context identifier */
  externalName: string;
  
  /** Subgraph internal context name */
  internalName: string;
  
  /** Whether this input is required */
  required?: boolean;
  
  /** Description for documentation */
  description?: string;
}

/**
 * SUBGRAPH_START Node Configuration
 * 
 * This node replaces the original START node in subgraphs after expansion.
 * It handles:
 * 1. Variable input mapping from parent workflow
 * 2. Entering variable scope for isolation
 * 3. Message context passing
 * 
 * IMPORTANT: This configuration is generated during preprocessing and should NOT
 * be manually defined in workflow TOML files.
 */
export interface SubgraphStartNodeConfig {
  /**
   * Variable Inputs - Mapped from parent SUBGRAPH node's variableInputs
   * These define which parent variables are accessible in this subgraph
   */
  variableInputs?: SubgraphStartVariableInput[];
  
  /**
   * Message Context Inputs
   */
  messageInputs?: SubgraphStartMessageInput[];
  
  /**
   * Original subgraph ID (for debugging/tracing)
   */
  originalSubgraphId?: ID;
}

// ============================================================================
// SUBGRAPH_END Node Configuration
// ============================================================================

/**
 * Variable Output Mapping for SUBGRAPH_END node
 * 
 * Defines how subgraph internal variables are returned to parent workflow.
 */
export interface SubgraphEndVariableOutput {
  /** Subgraph internal variable name (source) */
  internalName: string;
  
  /** Parent workflow variable name (target) */
  externalName: string;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Message Context Output Mapping for SUBGRAPH_END node
 */
export interface SubgraphEndMessageOutput {
  /** Subgraph internal context name */
  internalName: string;
  
  /** Parent context identifier */
  externalName: string;
  
  /** Description for documentation */
  description?: string;
}

/**
 * SUBGRAPH_END Node Configuration
 * 
 * This node replaces the original END node in subgraphs after expansion.
 * It handles:
 * 1. Exiting variable scope (discarding local variables)
 * 2. Collecting output variables (if configured)
 * 3. Message context returning
 * 
 * IMPORTANT: This configuration is generated during preprocessing and should NOT
 * be manually defined in workflow TOML files.
 */
export interface SubgraphEndNodeConfig {
  /**
   * Variable Outputs - Optional explicit output mapping
   * If not provided, all execution-scope variables are discarded
   */
  variableOutputs?: SubgraphEndVariableOutput[];
  
  /**
   * Message Context Outputs
   */
  messageOutputs?: SubgraphEndMessageOutput[];
  
  /**
   * Original subgraph ID (for debugging/tracing)
   */
  originalSubgraphId?: ID;
}
