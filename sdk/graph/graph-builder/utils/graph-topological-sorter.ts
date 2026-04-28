/**
 * Graph Topological Sort Tool Function
 * Provides the topological sorting algorithm for graphs (Kahn's algorithm)
 */

import type { ID, TopologicalSortResult, Graph } from "@wf-agent/types";
import { detectCycles } from "./graph-cycle-detector.js";

/**
 * Topological Sort (Kahn's Algorithm)
 * @param graph - The graph data to be sorted
 * @returns The result of the topological sort
 */
export function topologicalSort(graph: Graph): TopologicalSortResult {
  const sortedNodes: ID[] = [];
  const inDegree = new Map<ID, number>();
  const queue: ID[] = [];

  // Calculate the in-degree of each node.
  for (const nodeId of graph.getAllNodeIds()) {
    inDegree.set(nodeId, graph.getIncomingEdges(nodeId).length);
  }

  // Add nodes with an in-degree of 0 to the queue.
  for (const [nodeId, degree] of inDegree) {
    if (degree === 0) {
      queue.push(nodeId);
    }
  }

  // Processing queue
  while (queue.length > 0) {
    const nodeId = queue.shift()!;
    sortedNodes.push(nodeId);

    // Reduce the in-degree of neighboring nodes.
    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      const newDegree = inDegree.get(neighborId)! - 1;
      inDegree.set(neighborId, newDegree);
      if (newDegree === 0) {
        queue.push(neighborId);
      }
    }
  }

  // Check whether all nodes are sorted.
  const hasCycle = sortedNodes.length !== graph.getNodeCount();

  return {
    success: !hasCycle,
    sortedNodes,
    cycleNodes: hasCycle ? findCycleNodes(graph) : undefined,
  };
}

/**
 * Find the nodes in a cycle (used when topological sorting fails)
 * @param graph - Graph data
 * @returns List of node IDs in the cycle
 */
function findCycleNodes(graph: Graph): ID[] {
  const cycleResult = detectCycles(graph);
  return cycleResult.cycleNodes || [];
}
