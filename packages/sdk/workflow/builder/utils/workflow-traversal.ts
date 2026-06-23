/**
 * Workflow Graph Traversal Utility Functions
 * Provides depth-first traversal algorithms for workflow graphs
 */

import type { ID, WorkflowGraphStructure } from "@wf-agent/types";

/**
 * Depth-First Search with custom visitor and state management
 * This is the core DFS implementation used by all traversal functions
 *
 * @param graph - The graph data to be traversed
 * @param startNodeId - The ID of the starting node
 * @param options - DFS options
 * @param options.visitor - Called when visiting a node (before exploring neighbors)
 * @param options.useReverseEdges - If true, traverse using incoming edges instead of outgoing
 * @returns Set of visited node IDs
 */
function dfsCore(
  graph: WorkflowGraphStructure,
  startNodeId: ID,
  options: {
    visitor?: (nodeId: ID) => void;
    useReverseEdges?: boolean;
  } = {},
): Set<ID> {
  const { visitor, useReverseEdges = false } = options;

  // Check if the starting node exists.
  if (!graph.hasNode(startNodeId)) {
    return new Set<ID>();
  }

  const visited = new Set<ID>();
  const stack = [startNodeId];

  while (stack.length > 0) {
    const nodeId = stack.pop()!;
    if (visited.has(nodeId)) {
      continue;
    }

    visited.add(nodeId);

    // Call visitor if provided
    if (visitor) {
      visitor(nodeId);
    }

    // Get neighbors based on edge direction
    const neighbors = useReverseEdges
      ? graph.getIncomingNeighbors(nodeId)
      : graph.getOutgoingNeighbors(nodeId);

    // Add the unvisited neighbor nodes to the stack.
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        stack.push(neighborId);
      }
    }
  }

  return visited;
}

/**
 * Get all nodes reachable from the specified node
 * Uses depth-first search internally
 *
 * @param graph - Graph data
 * @param startNodeId - ID of the starting node
 * @returns Set of IDs of the reachable nodes
 */
export function getReachableNodes(graph: WorkflowGraphStructure, startNodeId: ID): Set<ID> {
  return dfsCore(graph, startNodeId, { useReverseEdges: false });
}

/**
 * Get all nodes that can reach the specified target node
 * Uses reverse depth-first search (traverses incoming edges)
 *
 * @param graph - Graph data
 * @param targetNodeId - Target node ID
 * @returns Set of node IDs that can reach the target node
 */
export function getNodesReachingTo(graph: WorkflowGraphStructure, targetNodeId: ID): Set<ID> {
  return dfsCore(graph, targetNodeId, { useReverseEdges: true });
}

/**
 * DFS callback for cycle detection
 * Called during recursive DFS traversal for cycle detection
 */
export type DfsCycleCallback = (
  nodeId: ID,
  path: ID[],
  visited: Set<ID>,
  recursionStack: Set<ID>,
) => {
  shouldContinue: boolean; // If false, stop traversing this branch
  foundCycle?: boolean; // If true, a cycle was detected
};

/**
 * Recursive DFS with path tracking for advanced algorithms (e.g., cycle detection)
 * This provides a standardized recursive DFS implementation to avoid code duplication
 *
 * @param graph - The graph data to traverse
 * @param startNodeId - Starting node ID
 * @param callback - Called for each node with current state
 * @param externalVisited - Optional external visited set shared across multiple DFS calls
 * @returns True if traversal should continue, false if stopped early
 */
export function dfsWithPathTracking(
  graph: WorkflowGraphStructure,
  startNodeId: ID,
  callback: DfsCycleCallback,
  externalVisited?: Set<ID>,
): void {
  const visited = externalVisited || new Set<ID>();
  const recursionStack = new Set<ID>();

  const dfsRecursive = (nodeId: ID, path: ID[]): boolean => {
    // Call the callback to check if we should process this node
    const result = callback(nodeId, path, visited, recursionStack);

    if (result.foundCycle) {
      return false; // Stop traversal if cycle found
    }

    visited.add(nodeId);
    recursionStack.add(nodeId);
    path.push(nodeId);

    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        if (!dfsRecursive(neighborId, path)) {
          return false;
        }
      } else if (recursionStack.has(neighborId)) {
        // Found a back edge (cycle)
        const cycleResult = callback(neighborId, path, visited, recursionStack);
        if (!cycleResult.shouldContinue) {
          return false;
        }
      }
    }

    recursionStack.delete(nodeId);
    path.pop();
    return true;
  };

  // Start DFS from the given node
  if (!visited.has(startNodeId)) {
    dfsRecursive(startNodeId, []);
  }
}

/**
 * DFS with path tracking that supports early termination and shared state
 * Similar to dfsWithPathTracking but returns a boolean indicating if cycle was found
 *
 * @param graph - The graph data to traverse
 * @param startNodeId - Starting node ID
 * @param callback - Called for each node with current state
 * @param visited - Shared visited set across multiple calls
 * @returns True if a cycle was found, false otherwise
 */
export function dfsWithPathTrackingAndEarlyExit(
  graph: WorkflowGraphStructure,
  startNodeId: ID,
  callback: DfsCycleCallback,
  visited: Set<ID>,
): boolean {
  const recursionStack = new Set<ID>();

  const dfsRecursive = (nodeId: ID, path: ID[]): boolean => {
    // Call the callback to check if we should process this node
    const result = callback(nodeId, path, visited, recursionStack);

    if (result.foundCycle) {
      return true; // Cycle found, propagate upward
    }

    visited.add(nodeId);
    recursionStack.add(nodeId);
    path.push(nodeId);

    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        if (dfsRecursive(neighborId, path)) {
          return true; // Cycle found, propagate upward
        }
      } else if (recursionStack.has(neighborId)) {
        // Found a back edge (cycle)
        const cycleResult = callback(neighborId, path, visited, recursionStack);
        if (cycleResult.foundCycle) {
          return true; // Cycle found, propagate upward
        }
      }
    }

    recursionStack.delete(nodeId);
    path.pop();
    return false; // No cycle found in this branch
  };

  // Start DFS from the given node
  if (!visited.has(startNodeId)) {
    return dfsRecursive(startNodeId, []);
  }

  return false;
}
