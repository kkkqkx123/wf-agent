/**
 * Workflow Graph Interface
 * Extending the Graph interface to add preprocessing-related metadata
 */

import type { WorkflowGraphStructure } from "./structure.js";
import type { ID, Timestamp, Version } from "../common.js";
import type { IdMapping, SubgraphRelationship } from "../workflow/id-mapping.js";
import type { WorkflowGraphAnalysis } from "./analysis.js";
import type { PreprocessValidationResult, SubgraphMergeLog } from "../workflow/preprocess.js";
import type { WorkflowTrigger } from "../trigger/index.js";
import type { VariableDefinition } from "../workflow-execution/variables.js";
import type { AvailableTools } from "../available-tools.js";
import type { StaticNode } from "../node/index.js";

/**
 * Workflow Graph Interface
 * Extends the Graph interface with all preprocessing-related information
 */
export interface WorkflowGraph extends WorkflowGraphStructure {
  // ========== ID mapping correlation =======
  /** ID mapping table (temporary data for build phase) */
  idMapping: IdMapping;

  /** 
   * Pre-processed node configuration (with updated ID references)
   * Maps node ID to its static node configuration with resolved references
   */
  nodeConfigs: Map<ID, StaticNode>;

  /** 
   * Preprocessed Trigger Configuration (with updated ID references)
   * Maps trigger ID to its workflow trigger configuration
   */
  triggerConfigs: Map<ID, WorkflowTrigger>;

  /** Sub-workflow relationships */
  subgraphRelationships: SubgraphRelationship[];

  // ========== Preprocessing metadata ==========
  /** Graphical analysis results */
  graphAnalysis: WorkflowGraphAnalysis;

  /** Preprocessing validation results */
  validationResult: PreprocessValidationResult;

  /** List of node IDs after topology sorting */
  topologicalOrder: ID[];

  /** Sub-workflow merge log */
  subgraphMergeLogs: SubgraphMergeLog[];

  /** preprocessing timestamp */
  processedAt: Timestamp;

  // ========== Workflow Metadata ==========
  /** Workflow ID */
  workflowId: ID;

  /** Workflow version */
  workflowVersion: Version;

  /** Triggers (expanded, no references included) */
  triggers?: WorkflowTrigger[];

  /** Workflow variable definitions */
  variables?: VariableDefinition[];

  /** Whether or not it contains sub workflows */
  hasSubgraphs: boolean;

  /** A collection of sub-workflow IDs */
  subworkflowIds: Set<ID>;

  /**
   * Available tool configurations
   * 
   * Unified interface for specifying which tools are available during workflow execution.
   */
  availableTools?: AvailableTools;
}
