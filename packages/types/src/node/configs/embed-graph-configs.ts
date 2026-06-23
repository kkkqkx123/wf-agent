/**
 * EmbedGraph Node Configuration Type Definition
 * 
 * Configuration for EMBED_GRAPH nodes that perform lightweight graph expansion
 * for pure control flow reuse WITHOUT variable isolation.
 * 
 * Design Philosophy:
 * - Zero overhead: No independent execution entity creation
 * - Pure control flow template reuse
 * - NO variable isolation (shares parent's VariableManager)
 * - Strict validation: Cannot contain variables, triggers, or VARIABLE nodes
 * - Performance optimization for high-frequency small subgraphs
 * 
 * Use Cases:
 * - Error handling templates
 * - Retry logic patterns
 * - Common branching structures
 * - Any subgraph that doesn't need variable isolation
 * 
 * IMPORTANT: EMBED_GRAPH is NOT a replacement for SUBGRAPH.
 * Use SUBGRAPH when you need variable isolation and explicit mapping.
 * Use EMBED_GRAPH only for performance-critical scenarios with simple control flow.
 */

import type { ID } from '../../common.js';

/**
 * EmbedGraph node configuration
 * 
 * Used for embedding workflows as lightweight control flow templates.
 * The embedded graph is expanded and merged into the parent graph during preprocessing,
 * similar to the old SUBGRAPH behavior but with strict constraints.
 * 
 * CONSTRAINTS (enforced by static validator):
 * 1. Embedded workflow CANNOT define any variables
 * 2. Embedded workflow CANNOT have any triggers
 * 3. Embedded workflow CANNOT contain VARIABLE nodes
 * 4. Embedded workflow CANNOT contain nested SUBGRAPH/EMBED_GRAPH with variables
 * 
 * Example:
 * ```toml
 * [[nodes]]
 * id = "handle_error"
 * type = "EMBED_GRAPH"
 * config.embedId = "error-handler-template"
 * ```
 */
export interface EmbedGraphNodeConfig {
  /** Embedded workflow ID (must reference a valid DEPENDENT workflow) */
  embedId: ID;
  
  /**
   * NOTE: EMBED_GRAPH does NOT support the following configurations:
   * - variableInputs/Outputs (no variable isolation)
   * - messagePassing (shares parent's message context)
   * - async option (always synchronous expansion)
   * 
   * If you need any of these features, use SUBGRAPH instead.
   */
}
