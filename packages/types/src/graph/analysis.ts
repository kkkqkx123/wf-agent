/**
 * Graph Analysis Result Type Definition
 */

import { NodeType } from "../node/index.js";
import type { EdgeType } from "../edge.js";
import type {
  CycleDetectionResult,
  ReachabilityResult,
  TopologicalSortResult,
  ForkJoinValidationResult,
} from "./validation.js";

/**
 * Graph Analysis Results
 * Contains the results of all graph analysis algorithms
 */
export interface GraphAnalysisResult {
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
    byType: Map<NodeType, number>;
  };
  /** Border statistical information */
  edgeStats: {
    /** total number of sides */
    total: number;
    /** Number of edges grouped by type */
    byType: Map<EdgeType, number>;
  };
}
