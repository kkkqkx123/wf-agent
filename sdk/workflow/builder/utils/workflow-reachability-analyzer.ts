/**
 * Workflow Graph Reachability Analysis Tool Functions
 * Provide algorithms for workflow graph reachability analysis
 */

import type { ID, ReachabilityResult, WorkflowGraphStructure } from "@wf-agent/types";
import { getReachableNodes, getNodesReachingTo } from "./workflow-traversal.js";

/**
 * Analyze the reachability of the workflow graph
 * @param graph - The graph data to be analyzed
 * @returns The reachability analysis results
 */
export function analyzeReachability(graph: WorkflowGraphStructure): ReachabilityResult {
  // Traverse from the START node in the forward direction (if it exists).
  const reachableFromStart = graph.startNodeId
    ? getReachableNodes(graph, graph.startNodeId)
    : new Set<ID>();

  // Traverse backwards from the END node.
  const reachableToEnd = new Set<ID>();
  for (const endNodeId of graph.endNodeIds) {
    const reachingNodes = getNodesReachingTo(graph, endNodeId);
    for (const nodeId of reachingNodes) {
      reachableToEnd.add(nodeId);
    }
  }

  // Find unreachable nodes (nodes that cannot be reached from START).
  const unreachableNodes = new Set<ID>();
  for (const nodeId of graph.getAllNodeIds()) {
    if (!reachableFromStart.has(nodeId)) {
      unreachableNodes.add(nodeId);
    }
  }

  // Find dead nodes (nodes that are reachable from START but cannot be reached from END).
  const deadEndNodes = new Set<ID>();
  for (const nodeId of graph.getAllNodeIds()) {
    if (reachableFromStart.has(nodeId) && !reachableToEnd.has(nodeId)) {
      deadEndNodes.add(nodeId);
    }
  }

  return {
    reachableFromStart,
    reachableToEnd,
    unreachableNodes,
    deadEndNodes,
  };
}
