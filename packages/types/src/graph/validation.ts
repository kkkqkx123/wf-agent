/**
 * Graph Validation Related Type Definitions
 */

import type { ID } from "../common.js";

/**
 * Ring test results
 */
export interface CycleDetectionResult {
  /** Whether or not there is a ring */
  hasCycle: boolean;
  /** List of node IDs in the ring (if there is a ring) */
  cycleNodes?: ID[];
  /** List of edge IDs in the ring (if there is a ring) */
  cycleEdges?: ID[];
}

/**
 * Results of accessibility analysis
 */
export interface ReachabilityResult {
  /** The set of nodes reachable from the START node */
  reachableFromStart: Set<ID>;
  /** The set of nodes that can reach the END node */
  reachableToEnd: Set<ID>;
  /** Unreachable nodes (unreachable from START) */
  unreachableNodes: Set<ID>;
  /** Dead node (unable to reach END) */
  deadEndNodes: Set<ID>;
}

/**
 * Topological ordering results
 */
export interface TopologicalSortResult {
  /** Successful sorting or not (no loop) */
  success: boolean;
  /** List of node IDs after topology sorting */
  sortedNodes: ID[];
  /** If there is a ring, the nodes in the ring */
  cycleNodes?: ID[];
}

/**
 * FORK/JOIN pairwise validation results
 */
export interface ForkJoinValidationResult {
  /** Whether the validation passes or not */
  isValid: boolean;
  /** Unpaired FORK nodes */
  unpairedForks: ID[];
  /** Unpaired JOIN nodes */
  unpairedJoins: ID[];
  /** Pairing details */
  pairs: Map<ID, ID>;
}

/**
 * Graph Validation Options
 */
export interface GraphValidationOptions {
  /** Whether to detect the ring */
  checkCycles?: boolean;
  /** Whether to check accessibility */
  checkReachability?: boolean;
  /** Whether to check FORK/JOIN pairing */
  checkForkJoin?: boolean;
  /** Whether to check START/END nodes */
  checkStartEnd?: boolean;
  /** Whether to check for isolated nodes */
  checkIsolatedNodes?: boolean;
  /** Whether to check for sub workflow existence */
  checkSubgraphExistence?: boolean;
  /** Whether to check sub workflow interface compatibility */
  checkSubgraphCompatibility?: boolean;
}
