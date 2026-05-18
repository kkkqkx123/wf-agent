/**
 * Workflow Builder Module
 *
 * Provides tools for building, navigating, and analyzing workflow graphs.
 */

// Graph Builder
export { WorkflowGraphBuilder } from "./workflow-graph-builder.js";

// Graph Navigator
export {
  WorkflowNavigator,
  type NavigationResult,
  type RoutingDecision,
  type PathEnumerationOptions,
  type PathEnumerationResult,
} from "./workflow-navigator.js";

// Graph Analysis Utilities
export { detectCycles } from "./utils/workflow-cycle-detector.js";
export { analyzeWorkflowGraph } from "./utils/workflow-graph-analyzer.js";
export { topologicalSort } from "./utils/workflow-topological-sorter.js";
export { analyzeReachability } from "./utils/workflow-reachability-analyzer.js";

// Graph Traversal Utilities
export {
  getReachableNodes,
  getNodesReachingTo,
  dfsWithPathTracking,
  dfsWithPathTrackingAndEarlyExit,
  type DfsCycleCallback,
} from "./utils/workflow-traversal.js";
