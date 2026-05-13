/**
 * Workflow Graph Loop Detection Tool Function
 * Provides an algorithm for detecting loops in workflow graphs
 */

import type { ID, CycleDetectionResult, WorkflowGraphStructure } from "@wf-agent/types";
import { dfsWithPathTracking, type DfsCycleCallback } from "./workflow-traversal.js";

/**
 * Detect cycles in a workflow graph (using DFS)
 * @param graph - The graph data to be checked
 * @returns The result of cycle detection
 */
export function detectCycles(graph: WorkflowGraphStructure): CycleDetectionResult {
  const cycleNodes: ID[] = [];
  const cycleEdges: ID[] = [];
  let hasCycle = false;

  // Define the callback for cycle detection
  const cycleCallback: DfsCycleCallback = (nodeId, path, visited, recursionStack) => {
    if (recursionStack.has(nodeId)) {
      // Found a loop
      hasCycle = true;
      const cycleStartIndex = path.indexOf(nodeId);
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

      return { shouldContinue: false, foundCycle: true };
    }

    return { shouldContinue: true };
  };

  // Start a DFS from each unvisited node.
  for (const nodeId of graph.getAllNodeIds()) {
    dfsWithPathTracking(graph, nodeId, cycleCallback);
    if (hasCycle) {
      break;
    }
  }

  return {
    hasCycle,
    cycleNodes: hasCycle ? cycleNodes : undefined,
    cycleEdges: hasCycle ? cycleEdges : undefined,
  };
}
