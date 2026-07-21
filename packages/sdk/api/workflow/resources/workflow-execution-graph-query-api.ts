/**
 * WorkflowExecutionGraphQueryAPI - Workflow Execution Decision Graph Query API
 *
 * Provides execution graph and decision path analysis for workflow executions.
 * Mirrors the Agent's AgentExecutionGraphQueryAPI for workflow context.
 * Tracks node execution decisions, execution paths, and alternative decision analysis.
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * Execution node in the workflow graph
 */
export interface WorkflowExecutionNode {
  /** Node ID */
  id: string;
  /** Node name */
  name?: string;
  /** Node type */
  nodeType: string;
  /** Execution iteration/order */
  order: number;
  /** Whether execution was successful */
  success: boolean;
  /** Execution duration (ms) */
  duration?: number;
  /** Error message if failed */
  error?: string;
  /** Timestamp when execution started */
  startedAt: number;
  /** Timestamp when execution completed */
  completedAt?: number;
  /** Decision confidence (0-1) if applicable */
  confidence?: number;
  /** Alternative node IDs that were considered */
  alternatives?: string[];
}

/**
 * Execution edge between nodes
 */
export interface WorkflowExecutionEdge {
  /** Source node ID */
  fromNodeId: string;
  /** Target node ID */
  toNodeId: string;
  /** Edge label/condition */
  label?: string;
  /** Whether this edge was taken */
  taken: boolean;
  /** Transition duration (ms) */
  duration?: number;
  /** Probability of this path being chosen (0-1) */
  probability?: number;
  /** Confidence in this decision (0-1) */
  confidence?: number;
  /** Alternative target node IDs that were considered */
  alternatives?: string[];
  /** Reason for choosing this path */
  decisionReason?: string;
}

/**
 * Execution graph for a workflow execution
 */
export interface WorkflowExecutionGraph {
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Nodes in execution order */
  nodes: WorkflowExecutionNode[];
  /** Edges between nodes */
  edges: WorkflowExecutionEdge[];
  /** Start node ID */
  startNodeId: string;
  /** End node ID */
  endNodeId: string;
  /** IDs of nodes that errored */
  errorNodeIds: string[];
  /** Graph density metric */
  graphDensity?: number;
}

/**
 * Execution path summary
 */
export interface WorkflowExecutionPathSummary {
  /** Execution ID */
  executionId: ID;
  /** Node IDs in execution order */
  nodeIds: string[];
  /** Total duration (ms) */
  totalDuration: number;
  /** Number of steps */
  stepsCount: number;
  /** Number of retries/failures */
  failureCount: number;
  /** Complexity score */
  complexityScore: number;
  /** Optimality score (0-1) */
  optimalityScore: number;
}

/**
 * Execution graph filter options
 */
export interface WorkflowExecutionGraphFilter {
  /** Filter by execution ID */
  executionId?: ID;
  /** Filter by workflow ID */
  workflowId?: ID;
  /** Minimum nodes */
  minNodes?: number;
  /** Maximum nodes */
  maxNodes?: number;
}

// ============================================================================
// Type Definitions: Decision Analysis
// ============================================================================

/**
 * Alternative decision at a decision point
 */
export interface AlternativeDecision {
  /** Alternative node ID */
  nodeId: string;
  /** Alternative node name */
  nodeName?: string;
  /** Description of the alternative */
  description: string;
  /** Why this alternative was considered */
  reason?: string;
  /** Estimated outcome if chosen */
  estimatedOutcome?: string;
  /** Success probability (0-1) */
  successProbability?: number;
  /** Confidence in this estimate (0-1) */
  confidence?: number;
  /** Pros of this alternative */
  pros?: string[];
  /** Cons of this alternative */
  cons?: string[];
}

/**
 * Decision point analysis
 */
export interface DecisionPoint {
  /** Node ID where decision was made */
  nodeId: string;
  /** Node name */
  nodeName?: string;
  /** Timestamp of decision */
  timestamp: number;
  /** Decision that was actually taken */
  chosenPath: {
    targetNodeId: string;
    description: string;
    reasoning?: string;
  };
  /** Other alternatives considered */
  alternatives: AlternativeDecision[];
  /** Total alternatives evaluated */
  totalAlternatives: number;
  /** Confidence in the chosen path (0-1) */
  confidence?: number;
}

/**
 * Complete decision analysis for an execution
 */
export interface WorkflowDecisionAnalysis {
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Total decision points */
  totalDecisionPoints: number;
  /** All decision points identified */
  decisionPoints: DecisionPoint[];
  /** Average confidence across all decisions */
  averageConfidence: number;
  /** Most common decision type */
  mostCommonDecisionType: string;
  /** Decision frequency by type */
  decisionFrequency: Record<string, number>;
}

/**
 * Path probability analysis
 */
export interface PathProbabilityAnalysis {
  /** Execution ID */
  executionId: ID;
  /** All possible paths with probabilities */
  paths: Array<{
    pathId: string;
    nodeIds: string[];
    probability: number;
    isTaken: boolean;
  }>;
  /** Most likely path */
  mostLikelyPath: string[] | null;
  /** Least likely path taken */
  leastLikelyTakenPath: string[] | null;
  /** Path diversity score (0-1) */
  pathDiversity: number;
}

// ============================================================================
// WorkflowExecutionGraphQueryAPI
// ============================================================================

/**
 * WorkflowExecutionGraphQueryAPI - Execution decision graph queries
 * Provides graph-based analysis of workflow execution paths and decisions.
 */
export class WorkflowExecutionGraphQueryAPI extends QueryableResourceAPI<
  WorkflowExecutionGraph,
  string,
  WorkflowExecutionGraphFilter
> {
  private executionGraphs: Map<string, WorkflowExecutionGraph> = new Map();
  private executionPaths: Map<string, WorkflowExecutionPathSummary> = new Map();

  constructor(deps: APIDependencyManager) {
    super();
    void deps;
  }

  // ============================================================================
  // Abstract Method Implementations
  // ============================================================================

  protected override async getResource(id: string): Promise<WorkflowExecutionGraph | null> {
    return this.executionGraphs.get(id) ?? null;
  }

  protected override async getAllResources(): Promise<WorkflowExecutionGraph[]> {
    return Array.from(this.executionGraphs.values());
  }

  protected override applyFilter(
    records: WorkflowExecutionGraph[],
    filter: WorkflowExecutionGraphFilter,
  ): WorkflowExecutionGraph[] {
    let filtered = records;

    if (filter.executionId) {
      filtered = filtered.filter((r) => r.executionId === filter.executionId);
    }
    if (filter.workflowId) {
      filtered = filtered.filter((r) => r.workflowId === filter.workflowId);
    }
    if (filter.minNodes !== undefined) {
      filtered = filtered.filter((r) => r.nodes.length >= filter.minNodes!);
    }
    if (filter.maxNodes !== undefined) {
      filtered = filtered.filter((r) => r.nodes.length <= filter.maxNodes!);
    }

    return filtered;
  }

  // ============================================================================
  // Graph Data Management
  // ============================================================================

  /**
   * Record an execution graph for a workflow execution
   * @param graph Execution graph data
   */
  recordExecutionGraph(graph: WorkflowExecutionGraph): void {
    this.executionGraphs.set(graph.executionId as string, graph);
  }

  /**
   * Record an execution path summary
   * @param path Execution path summary
   */
  recordExecutionPath(path: WorkflowExecutionPathSummary): void {
    this.executionPaths.set(path.executionId as string, path);
  }

  // ============================================================================
  // Execution Graph Queries
  // ============================================================================

  /**
   * Get execution graph for a workflow execution
   * @param executionId Execution ID
   * @returns Execution graph or null
   */
  async getExecutionGraph(executionId: ID): Promise<WorkflowExecutionGraph | null> {
    return this.executionGraphs.get(executionId as string) ?? null;
  }

  /**
   * Get execution nodes for a workflow execution
   * @param executionId Execution ID
   * @returns Array of execution nodes
   */
  async getExecutionNodes(executionId: ID): Promise<WorkflowExecutionNode[]> {
    const graph = this.executionGraphs.get(executionId as string);
    return graph?.nodes ?? [];
  }

  /**
   * Get execution edges for a workflow execution
   * @param executionId Execution ID
   * @returns Array of execution edges
   */
  async getExecutionEdges(executionId: ID): Promise<WorkflowExecutionEdge[]> {
    const graph = this.executionGraphs.get(executionId as string);
    return graph?.edges ?? [];
  }

  /**
   * Get outgoing edges from a node
   * @param executionId Execution ID
   * @param nodeId Source node ID
   * @returns Outgoing edges
   */
  async getOutgoingEdges(executionId: ID, nodeId: string): Promise<WorkflowExecutionEdge[]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return [];
    return graph.edges.filter((e) => e.fromNodeId === nodeId);
  }

  /**
   * Get incoming edges to a node
   * @param executionId Execution ID
   * @param nodeId Target node ID
   * @returns Incoming edges
   */
  async getIncomingEdges(executionId: ID, nodeId: string): Promise<WorkflowExecutionEdge[]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return [];
    return graph.edges.filter((e) => e.toNodeId === nodeId);
  }

  /**
   * Get all execution paths from start to end
   * @param executionId Execution ID
   * @returns Array of paths (each path is an array of node IDs)
   */
  async getAllPaths(executionId: ID): Promise<string[][]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return [];

    const paths: string[][] = [];
    const visited = new Set<string>();
    const currentPath: string[] = [];

    const dfs = (nodeId: string) => {
      if (visited.has(nodeId)) return;
      visited.add(nodeId);
      currentPath.push(nodeId);

      if (nodeId === graph.endNodeId || graph.errorNodeIds?.includes(nodeId)) {
        paths.push([...currentPath]);
      } else {
        const outgoing = graph.edges.filter((e) => e.fromNodeId === nodeId);
        for (const edge of outgoing) {
          dfs(edge.toNodeId);
        }
      }

      currentPath.pop();
      visited.delete(nodeId);
    };

    dfs(graph.startNodeId);
    return paths;
  }

  // ============================================================================
  // Execution Path Queries
  // ============================================================================

  /**
   * Get execution path summary
   * @param executionId Execution ID
   * @returns Execution path summary or null
   */
  async getExecutionPathSummary(executionId: ID): Promise<WorkflowExecutionPathSummary | null> {
    return this.executionPaths.get(executionId as string) ?? null;
  }

  /**
   * Get path statistics
   * @param executionId Execution ID
   * @returns Path analysis statistics or null
   */
  async getPathStatistics(
    executionId: ID,
  ): Promise<{
    stepsCount: number;
    totalDuration: number;
    averageNodeDuration: number;
    complexityScore: number;
    optimalityScore: number;
    failureRate: number;
  } | null> {
    const path = this.executionPaths.get(executionId as string);
    if (!path) return null;

    const avgNodeDuration =
      path.stepsCount > 0 ? path.totalDuration / path.stepsCount : 0;

    return {
      stepsCount: path.stepsCount,
      totalDuration: path.totalDuration,
      averageNodeDuration: avgNodeDuration,
      complexityScore: path.complexityScore,
      optimalityScore: path.optimalityScore,
      failureRate: path.stepsCount > 0 ? path.failureCount / path.stepsCount : 0,
    };
  }

  /**
   * Get failed nodes in an execution
   * @param executionId Execution ID
   * @returns Array of failed nodes
   */
  async getFailedNodes(executionId: ID): Promise<WorkflowExecutionNode[]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return [];
    return graph.nodes.filter((n) => !n.success);
  }

  /**
   * Get slow nodes (above percentile threshold)
   * @param executionId Execution ID
   * @param percentile Duration percentile threshold (default: 0.8 = top 20%)
   * @returns Array of slow nodes
   */
  async getSlowNodes(
    executionId: ID,
    percentile: number = 0.8,
  ): Promise<WorkflowExecutionNode[]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph || graph.nodes.length === 0) return [];

    const durations = graph.nodes
      .map((n) => n.duration ?? 0)
      .filter((d) => d > 0)
      .sort((a, b) => a - b);

    if (durations.length === 0) return [];

    const thresholdIndex = Math.floor(durations.length * percentile);
    const threshold = durations[Math.min(thresholdIndex, durations.length - 1)];

    const thresholdValue = threshold ?? 0;
    return graph.nodes.filter((n) => (n.duration ?? 0) > thresholdValue);
  }

  // ============================================================================
  // Graph Analysis
  // ============================================================================

  /**
   * Analyze execution efficiency
   * @param executionId Execution ID
   * @returns Efficiency analysis or null
   */
  async analyzeEfficiency(
    executionId: ID,
  ): Promise<{
    executedSteps: number;
    optimalSteps: number;
    efficiencyRatio: number;
    wastefulNodes: number;
    retryCount: number;
  } | null> {
    const path = this.executionPaths.get(executionId as string);
    const graph = this.executionGraphs.get(executionId as string);

    if (!path || !graph) return null;

    const allPaths = await this.getAllPaths(executionId);
    const shortestPathLength =
      allPaths.length > 0 ? Math.min(...allPaths.map((p) => p.length)) : path.stepsCount;

    return {
      executedSteps: path.stepsCount,
      optimalSteps: shortestPathLength,
      efficiencyRatio: shortestPathLength > 0 ? path.stepsCount / shortestPathLength : 1,
      wastefulNodes: Math.max(0, path.stepsCount - shortestPathLength),
      retryCount: path.failureCount,
    };
  }

  /**
   * Analyze node type distribution
   * @param executionId Execution ID
   * @returns Node type distribution
   */
  async getNodeTypeDistribution(
    executionId: ID,
  ): Promise<Record<string, number> | null> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return null;

    const distribution: Record<string, number> = {};
    for (const node of graph.nodes) {
      distribution[node.nodeType] = (distribution[node.nodeType] || 0) + 1;
    }
    return distribution;
  }

  /**
   * Get the critical path (longest path) through the execution graph
   * @param executionId Execution ID
   * @returns Critical path node IDs or null
   */
  async getCriticalPath(executionId: ID): Promise<string[] | null> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return null;

    // Find the path with the longest total duration
    const allPaths = await this.getAllPaths(executionId);
    if (allPaths.length === 0) return null;

    let criticalPath: string[] = [];
    let maxDuration = 0;

    for (const path of allPaths) {
      const duration = path.reduce((sum, nodeId) => {
        const node = graph.nodes.find((n) => n.id === nodeId);
        return sum + (node?.duration ?? 0);
      }, 0);

      if (duration > maxDuration) {
        maxDuration = duration;
        criticalPath = path;
      }
    }

    return criticalPath;
  }

  /**
   * Clear all stored data for an execution
   * @param executionId Execution ID
   */
  clearExecutionData(executionId: ID): void {
    const key = executionId as string;
    this.executionGraphs.delete(key);
    this.executionPaths.delete(key);
  }

  // ============================================================================
  // Decision Analysis
  // ============================================================================

  /**
   * Analyze decisions made during execution
   * Identifies decision points and evaluates alternatives
   * @param executionId Execution ID
   * @returns Decision analysis or null
   */
  async analyzeDecisions(executionId: ID): Promise<WorkflowDecisionAnalysis | null> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return null;

    const decisionPoints: DecisionPoint[] = [];
    let totalConfidence = 0;

    // Find nodes with multiple outgoing edges (decision points)
    for (const node of graph.nodes) {
      const outgoingEdges = graph.edges.filter((e) => e.fromNodeId === node.id);
      const takenEdges = outgoingEdges.filter((e) => e.taken);

      if (outgoingEdges.length > 1 || takenEdges.length > 0) {
        const takenEdge = takenEdges[0];
        const alternatives: AlternativeDecision[] = [];

        // Build alternatives from edges not taken and edge alternatives
        for (const edge of outgoingEdges) {
          if (edge.alternatives) {
            for (const altNodeId of edge.alternatives) {
              const altNode = graph.nodes.find((n) => n.id === altNodeId);
              alternatives.push({
                nodeId: altNodeId,
                nodeName: altNode?.name,
                description: altNode
                  ? `Alternative path via ${altNode.name}`
                  : `Alternative path via ${altNodeId}`,
                confidence: altNode?.confidence,
              });
            }
          }

          if (!edge.taken) {
            const altNode = graph.nodes.find((n) => n.id === edge.toNodeId);
            alternatives.push({
              nodeId: edge.toNodeId,
              nodeName: altNode?.name,
              description: altNode
                ? `Path via ${altNode.name} (${edge.label ?? "no label"})`
                : `Path via ${edge.toNodeId}`,
              reason: edge.decisionReason,
              confidence: edge.confidence ?? node.confidence,
            });
          }
        }

        if (takenEdge) {
          const targetNode = graph.nodes.find((n) => n.id === takenEdge.toNodeId);
          decisionPoints.push({
            nodeId: node.id,
            nodeName: node.name,
            timestamp: node.startedAt,
            chosenPath: {
              targetNodeId: takenEdge.toNodeId,
              description: targetNode
                ? `Chose path to ${targetNode.name}`
                : `Chose path to ${takenEdge.toNodeId}`,
              reasoning: takenEdge.decisionReason,
            },
            alternatives,
            totalAlternatives: alternatives.length,
            confidence: takenEdge.confidence ?? node.confidence,
          });

          if (node.confidence !== undefined) {
            totalConfidence += node.confidence;
          }
        }
      }
    }

    // Calculate decision frequency by node type
    const decisionFrequency: Record<string, number> = {};
    for (const dp of decisionPoints) {
      const node = graph.nodes.find((n) => n.id === dp.nodeId);
      const type = node?.nodeType ?? "unknown";
      decisionFrequency[type] = (decisionFrequency[type] ?? 0) + 1;
    }

    // Find most common decision type
    const mostCommonDecisionType =
      Object.entries(decisionFrequency).sort((a, b) => b[1] - a[1])[0]?.[0] ?? "none";

    return {
      executionId: executionId as ID,
      workflowId: graph.workflowId,
      totalDecisionPoints: decisionPoints.length,
      decisionPoints,
      averageConfidence:
        decisionPoints.length > 0 ? totalConfidence / decisionPoints.length : 0,
      mostCommonDecisionType,
      decisionFrequency,
    };
  }

  /**
   * Get alternative paths that were considered but not taken
   * @param executionId Execution ID
   * @returns Array of alternative path descriptions
   */
  async getAlternativePaths(executionId: ID): Promise<AlternativeDecision[]> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return [];

    const alternatives: AlternativeDecision[] = [];

    for (const edge of graph.edges) {
      if (!edge.taken && edge.alternatives) {
        for (const altNodeId of edge.alternatives) {
          const altNode = graph.nodes.find((n) => n.id === altNodeId);
          alternatives.push({
            nodeId: altNodeId,
            nodeName: altNode?.name,
            description: altNode
              ? `Alternative path via ${altNode.name}`
              : `Alternative path via ${altNodeId}`,
            confidence: altNode?.confidence,
          });
        }
      }
    }

    return alternatives;
  }

  /**
   * Analyze path probabilities across the execution graph
   * @param executionId Execution ID
   * @returns Path probability analysis
   */
  async getPathProbabilityAnalysis(executionId: ID): Promise<PathProbabilityAnalysis | null> {
    const graph = this.executionGraphs.get(executionId as string);
    if (!graph) return null;

    const allPaths = await this.getAllPaths(executionId);
    const pathSummary = this.executionPaths.get(executionId as string);

    // Calculate probability for each path
    const paths = allPaths.map((nodeIds, index) => {
      // Calculate path probability as product of edge probabilities
      let probability = 1;
      for (let i = 0; i < nodeIds.length - 1; i++) {
        const edge = graph.edges.find(
          (e) => e.fromNodeId === nodeIds[i] && e.toNodeId === nodeIds[i + 1],
        );
        if (edge?.probability !== undefined) {
          probability *= edge.probability;
        }
      }

      // Determine if this path was taken
      const isTaken = pathSummary
        ? nodeIds.every((id) => pathSummary.nodeIds.includes(id))
        : false;

      return {
        pathId: `path-${index}`,
        nodeIds,
        probability,
        isTaken,
      };
    });

    // Sort by probability descending
    paths.sort((a, b) => b.probability - a.probability);

    const mostLikelyPath = paths[0]?.nodeIds ?? null;
    const takenPaths = paths.filter((p) => p.isTaken);

    // Calculate path diversity (normalized entropy of path probabilities)
    const totalProb = paths.reduce((sum, p) => sum + p.probability, 0);
    const pathDiversity =
      totalProb > 0
        ? -paths.reduce((sum, p) => {
            const normalizedP = p.probability / totalProb;
            return sum + (normalizedP > 0 ? normalizedP * Math.log2(normalizedP) : 0);
          }, 0) / Math.log2(paths.length)
        : 0;

    return {
      executionId: executionId as ID,
      paths,
      mostLikelyPath,
      leastLikelyTakenPath: takenPaths[takenPaths.length - 1]?.nodeIds ?? null,
      pathDiversity: Math.round(pathDiversity * 100) / 100,
    };
  }
}