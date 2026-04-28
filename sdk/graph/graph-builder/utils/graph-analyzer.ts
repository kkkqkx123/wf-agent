/**
 * Graph Analysis Tool Functions
 * Provide comprehensive graph analysis capabilities, integrating all analysis algorithms.
 */

import type {
  ID,
  NodeType,
  EdgeType,
  GraphAnalysisResult,
  ForkJoinValidationResult,
  Graph,
} from "@wf-agent/types";
import { detectCycles } from "./graph-cycle-detector.js";
import { analyzeReachability } from "./graph-reachability-analyzer.js";
import { topologicalSort } from "./graph-topological-sorter.js";

/**
 * Complete graph analysis
 * @param graph - The graph data to be analyzed
 * @returns The results of the graph analysis
 */
export function analyzeGraph(graph: Graph): GraphAnalysisResult {
  // Loop Detection
  const cycleDetection = detectCycles(graph);

  // Reachability Analysis
  const reachability = analyzeReachability(graph);

  // Topological Sort
  const topologicalSortResult = topologicalSort(graph);

  // FORK/JOIN pairing verification (only collects pairing information, no verification is performed)
  const forkJoinValidation = collectForkJoinPairs(graph);

  // Node Statistics
  const nodeStats = {
    total: graph.getNodeCount(),
    byType: new Map<NodeType, number>(),
  };
  for (const node of graph.nodes.values()) {
    const count = nodeStats.byType.get(node.type) || 0;
    nodeStats.byType.set(node.type, count + 1);
  }

  // Counting edges
  const edgeStats = {
    total: graph.getEdgeCount(),
    byType: new Map<EdgeType, number>(),
  };
  for (const edge of graph.edges.values()) {
    const count = edgeStats.byType.get(edge.type) || 0;
    edgeStats.byType.set(edge.type, count + 1);
  }

  return {
    cycleDetection,
    reachability,
    topologicalSort: topologicalSortResult,
    forkJoinValidation,
    nodeStats,
    edgeStats,
  };
}

/**
 * Collect FORK/JOIN pair information (only collect, do not validate)
 * @param graph - Graph data
 * @returns FORK/JOIN pair information
 */
export function collectForkJoinPairs(graph: Graph): ForkJoinValidationResult {
  const forkNodes = new Map<ID, ID>(); // forkId -> nodeId
  const joinNodes = new Map<ID, ID>(); // joinId -> nodeId
  const pairs = new Map<ID, ID>();

  // Collect all FORK and JOIN nodes.
  for (const node of graph.nodes.values()) {
    if (node.type === ("FORK" as NodeType)) {
      const forkId = (node.originalNode?.config as { forkId?: ID } | undefined)?.forkId;
      if (forkId) {
        forkNodes.set(forkId, node.id);
      }
    } else if (node.type === ("JOIN" as NodeType)) {
      const joinId = (node.originalNode?.config as { joinId?: ID } | undefined)?.joinId;
      if (joinId) {
        joinNodes.set(joinId, node.id);
      }
    }
  }

  // Check the pairing.
  const unpairedForks: ID[] = [];
  const unpairedJoins: ID[] = [];

  for (const [forkId, forkNodeId] of forkNodes) {
    if (joinNodes.has(forkId)) {
      pairs.set(forkNodeId, joinNodes.get(forkId)!);
    } else {
      unpairedForks.push(forkNodeId);
    }
  }

  for (const [joinId, joinNodeId] of joinNodes) {
    if (!forkNodes.has(joinId)) {
      unpairedJoins.push(joinNodeId);
    }
  }

  return {
    isValid: unpairedForks.length === 0 && unpairedJoins.length === 0,
    unpairedForks,
    unpairedJoins,
    pairs,
  };
}
