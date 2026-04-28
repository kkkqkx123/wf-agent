/**
 * Sub-Workflow Merge Related Type Definitions
 */

import type { ID } from "../common.js";

/**
 * Diagram Building Options
 */
export interface GraphBuildOptions {
  /** Whether or not to validate the graph structure */
  validate?: boolean;
  /** Whether to compute topological ordering */
  computeTopologicalOrder?: boolean;
  /** Whether to detect the ring */
  detectCycles?: boolean;
  /** Whether to analyze accessibility */
  analyzeReachability?: boolean;
  /** Maximum recursion depth */
  maxRecursionDepth?: number;
  /** Current recursion depth (used internally) */
  currentDepth?: number;
  /** Workflow registrar reference (for querying subworkflows) */
  workflowRegistry?: unknown;
}

/**
 * Sub-Workflow Merge Options
 */
export interface SubgraphMergeOptions {
  /** Node ID namespace prefix */
  nodeIdPrefix?: string;
  /** Side ID namespace prefix */
  edgeIdPrefix?: string;
  /** Whether to keep the original ID mapping */
  preserveIdMapping?: boolean;
}

/**
 * Sub-workflow merge results
 */
export interface SubgraphMergeResult {
  /** Whether the merger is successful or not */
  success: boolean;
  /** Merged node ID mapping */
  nodeIdMapping: Map<ID, ID>;
  /** Merged edge ID mapping */
  edgeIdMapping: Map<ID, ID>;
  /** List of node IDs added */
  addedNodeIds: ID[];
  /** List of new side IDs */
  addedEdgeIds: ID[];
  /** List of node IDs removed (SUBGRAPH nodes) */
  removedNodeIds: ID[];
  /** List of Removed Edge IDs */
  removedEdgeIds: ID[];
  /** error message */
  errors: string[];
  /** List of merged sub workflow IDs */
  subworkflowIds: ID[];
}
