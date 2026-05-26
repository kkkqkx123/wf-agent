/**
 * Graph Analysis Result Type Definition
 */

import type { EdgeType } from "@wf-agent/types";
import type {
  CycleDetectionResult,
  ReachabilityResult,
  TopologicalSortResult,
  ForkJoinValidationResult,
} from "./validation.js";

export type {
  CycleDetectionResult,
  ReachabilityResult,
  TopologicalSortResult,
  ForkJoinValidationResult,
} from "./validation.js";

/**
 * Workflow Graph Analysis Results
 * Contains the results of all graph analysis algorithms
 */
export interface WorkflowGraphAnalysis {
  /** Ring test results */
  cycleDetection: CycleDetectionResult;
  /** Results of accessibility analysis */
  reachability: ReachabilityResult;
  /** Topological ordering results */
  topologicalSort: TopologicalSortResult;
  /** FORK/JOIN pairwise validation results */
  forkJoinValidation: ForkJoinValidationResult;
  /** Node statistics */
  nodeStats: {
    /** Total nodes */
    total: number;
    /** Number of nodes grouped by type */
    byType: Map<string, number>; // Use string to accommodate both StaticNodeType and RuntimeNodeType
  };
  /** Border statistical information */
  edgeStats: {
    /** total number of sides */
    total: number;
    /** Number of edges grouped by type */
    byType: Map<EdgeType, number>;
  };
}