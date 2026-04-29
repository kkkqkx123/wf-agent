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
import type { WorkflowVariable } from "../workflow/variables.js";

/**
 * Workflow Graph Interface
 * Extends the Graph interface with all preprocessing-related information
 */
export interface WorkflowGraph extends WorkflowGraphStructure {
  // ========== ID mapping correlation =======
  /** ID mapping table (temporary data for build phase) */
  idMapping: IdMapping;

  /** Pre-processed node configuration (with updated ID references) */
  nodeConfigs: Map<ID, unknown>;

  /** Preprocessed Trigger Configuration (with updated ID references) */
  triggerConfigs: Map<ID, unknown>;

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
  variables?: WorkflowVariable[];

  /** Whether or not it contains sub workflows */
  hasSubgraphs: boolean;

  /** A collection of sub-workflow IDs */
  subworkflowIds: Set<ID>;

  /** Available tool configurations */
  availableTools?: {
    /** Initial set of available tools (tool ID or name) */
    initial: Set<string>;
  };
}
