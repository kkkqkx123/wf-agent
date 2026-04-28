/**
 * Graph Traversal Utility Functions
 * Provides depth-first and breadth-first traversal algorithms for graphs
 */

import type { ID, Graph } from "@wf-agent/types";

/**
 * Depth-First Traversal
 * @param graph - The graph data to be traversed
 * @param startNodeId - The ID of the starting node
 * @param visitor - The visit function, which is called when each node is visited
 */
export function dfs(graph: Graph, startNodeId: ID, visitor: (nodeId: ID) => void): void {
  // Check if the starting node exists.
  if (!graph.hasNode(startNodeId)) {
    return;
  }

  const visited = new Set<ID>();
  const stack = [startNodeId];

  while (stack.length > 0) {
    const nodeId = stack.pop()!;
    if (visited.has(nodeId)) {
      continue;
    }

    visited.add(nodeId);
    visitor(nodeId);

    // Add the unvisited neighbor nodes to the stack.
    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        stack.push(neighborId);
      }
    }
  }
}

/**
 * Breadth-First Traversal
 * @param graph - The graph data to be traversed
 * @param startNodeId - The ID of the starting node
 * @param visitor - The visit function, which is called when each node is visited
 */
export function bfs(graph: Graph, startNodeId: ID, visitor: (nodeId: ID) => void): void {
  // Check if the starting node exists.
  if (!graph.hasNode(startNodeId)) {
    return;
  }

  const visited = new Set<ID>();
  const queue: ID[] = [startNodeId];
  visited.add(startNodeId);

  while (queue.length > 0) {
    const nodeId = queue.shift()!;
    visitor(nodeId);

    // Add the unvisited neighbor nodes to the queue.
    const neighbors = graph.getOutgoingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        visited.add(neighborId);
        queue.push(neighborId);
      }
    }
  }
}

/**
 * Get all nodes reachable from the specified node
 * @param graph - Graph data
 * @param startNodeId - ID of the starting node
 * @returns Set of IDs of the reachable nodes
 */
export function getReachableNodes(graph: Graph, startNodeId: ID): Set<ID> {
  const reachable = new Set<ID>();
  dfs(graph, startNodeId, nodeId => {
    reachable.add(nodeId);
  });
  return reachable;
}

/**
 * Get all nodes that can reach the specified target node
 * @param graph - Graph data
 * @param targetNodeId - Target node ID
 * @returns Set of node IDs that can reach the target node
 */
export function getNodesReachingTo(graph: Graph, targetNodeId: ID): Set<ID> {
  // Check if the target node exists.
  if (!graph.hasNode(targetNodeId)) {
    return new Set<ID>();
  }

  const reaching = new Set<ID>();
  const visited = new Set<ID>();
  const stack = [targetNodeId];

  while (stack.length > 0) {
    const nodeId = stack.pop()!;
    if (visited.has(nodeId)) {
      continue;
    }

    visited.add(nodeId);
    reaching.add(nodeId);

    // Traverse the reverse adjacency list.
    const neighbors = graph.getIncomingNeighbors(nodeId);
    for (const neighborId of neighbors) {
      if (!visited.has(neighborId)) {
        stack.push(neighborId);
      }
    }
  }

  return reaching;
}
