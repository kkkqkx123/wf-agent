/**
 * Workflow preprocessing related type definitions
 * These types are used during the workflow registration phase
 */

import type { ID } from "@wf-agent/types";

/**
 * ID mapping table
 * Used to track ID changes during ID reassignment to ensure reference consistency
 */
export interface IdMapping {
  /** Node ID mapping (original ID → new index) */
  nodeIds: Map<ID, number>;
  /** Edge ID mapping (original ID → new index) */
  edgeIds: Map<ID, number>;
  /** Reverse Node ID Map (New Index → Original ID) */
  reverseNodeIds: Map<number, ID>;
  /** Reverse Edge ID Map (New Index → Original ID) */
  reverseEdgeIds: Map<number, ID>;
  /** Sub-workflow namespace mapping (node ID → namespace) */
  subgraphNamespaces: Map<ID, string>;
}

/**
 * Sub-workflow relationship
 * Describes the relationship between a sub-workflow and its parent workflow
 */
export interface SubgraphRelationship {
  /** Parent workflow ID */
  parentWorkflowId: ID;
  /** SUBGRAPH node ID */
  subgraphNodeId: ID;
  /** Sub-workflow ID */
  childWorkflowId: ID;
  /** Namespace identifiers used for ID renaming */
  namespace: string;
}

/**
 * Sub-workflow merge log
 * Records the details of a sub-workflow merge process
 */
export interface SubgraphMergeLog {
  /** Sub-workflow ID */
  subworkflowId: ID;
  /** Sub-workflow name */
  subworkflowName: string;
  /** SUBGRAPH node ID */
  subgraphNodeId: ID;
  /** Merged Node ID Mapping (Original ID → New ID) */
  nodeIdMapping: Map<ID, ID>;
  /** Merged Edge ID Mapping (Original ID → New ID) */
  edgeIdMapping: Map<ID, ID>;
  /** Input mapping relationship */
  inputMapping: Map<string, ID>;
  /** Output mapping relationship */
  outputMapping: Map<string, ID>;
  /** Merge timestamp */
  mergedAt: number;
}

/**
 * Preprocessing validation results
 */
export interface PreprocessValidationResult {
  /** Is it legal */
  isValid: boolean;
  /** Error message list */
  errors: string[];
  /** Warning message list */
  warnings: string[];
  /** validation timestamp */
  validatedAt: number;
}
