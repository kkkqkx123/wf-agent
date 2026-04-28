/**
 * Graph Loop Detection Tool Function
 * Provides an algorithm for detecting loops in graphs
 */

import type { ID, CycleDetectionResult, Graph } from "@wf-agent/types";

/**
 * Detect cycles in a graph (using DFS)
 * @param graph - The graph data to be checked
 * @returns The result of cycle detection
 */
export function detectCycles(graph: Graph): CycleDetectionResult {
  const visited = new Set<ID>();
  const recursionStack = new Set<ID>();
  const cycleNodes: ID[] = [];
  const cycleEdges: ID[] = [];
  let hasCycle = false;

  const dfs = (nodeId: ID, path: ID[]): boolean => {
    visited.add(nodeId);
    recursionStack.add(nodeId);
    path.push(nodeId);

    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        if (dfs(neighborId, path)) {
          return true;
        }
      } else if (recursionStack.has(neighborId)) {
        // Found a loop
        hasCycle = true;
        const cycleStartIndex = path.indexOf(neighborId);
        cycleNodes.push(...path.slice(cycleStartIndex));

        // Find the edges in the cycle.
        for (let i = 0; i < cycleNodes.length - 1; i++) {
          const node1 = cycleNodes[i];
          const node2 = cycleNodes[i + 1];
          if (node1 && node2) {
            const edge = graph.getEdgeBetween(node1, node2);
            if (edge) {
              cycleEdges.push(edge.id);
            }
          }
        }
        // Add the last edge (from the last node back to the first node).
        const lastNode = cycleNodes[cycleNodes.length - 1];
        const firstNode = cycleNodes[0];
        if (lastNode && firstNode) {
          const lastEdge = graph.getEdgeBetween(lastNode, firstNode);
          if (lastEdge) {
            cycleEdges.push(lastEdge.id);
          }
        }

        return true;
      }
    }

    recursionStack.delete(nodeId);
    path.pop();
    return false;
  };

  // Start a DFS from each unvisited node.
  for (const nodeId of graph.getAllNodeIds()) {
    if (!visited.has(nodeId)) {
      if (dfs(nodeId, [])) {
        break;
      }
    }
  }

  return {
    hasCycle,
    cycleNodes: hasCycle ? cycleNodes : undefined,
    cycleEdges: hasCycle ? cycleEdges : undefined,
  };
}
