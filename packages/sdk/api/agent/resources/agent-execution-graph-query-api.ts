/**
 * AgentExecutionGraphQueryAPI - Agent Execution Graph Query API
 *
 * Provides decision graph and execution path analysis for Agent Loop.
 * Enables comprehensive tracking of decision-making process and alternative paths.
 *
 * Features:
 * - Decision graph structure
 * - Execution path tracking
 * - Alternative decision analysis
 * - Decision sequence reconstruction
 * - Path probability and confidence metrics
 *
 * Phase 2 Implementation: Add decision graph query capabilities to Agent Loop
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";

// ============================================================================
// Type Definitions: Decision Graph
// ============================================================================

/**
 * Decision node in the execution graph
 */
export interface DecisionNode {
  /** Node ID */
  nodeId: string;
  /** Node type */
  type: "start" | "decision" | "action" | "tool_call" | "end" | "error";
  /** Node description */
  description: string;
  /** Iteration where this decision was made */
  iteration: number;
  /** Timestamp of decision */
  timestamp: number;
  /** Decision context */
  context?: Record<string, unknown>;
  /** Confidence in this decision (0-1) */
  confidence?: number;
}

/**
 * Edge in the execution graph (represents a transition)
 */
export interface DecisionEdge {
  /** Edge ID */
  edgeId: string;
  /** Source node ID */
  fromNodeId: string;
  /** Target node ID */
  toNodeId: string;
  /** Why this transition was chosen */
  reason?: string;
  /** Condition that triggered this transition */
  condition?: string;
  /** Whether this edge was actually taken in execution */
  wasTaken: boolean;
  /** Probability of this path (0-1) */
  probability?: number;
  /** Weight for path scoring */
  weight?: number;
}

/**
 * Complete decision graph
 */
export interface DecisionGraph {
  /** Agent loop ID */
  agentLoopId: ID;
  /** All decision nodes */
  nodes: DecisionNode[];
  /** All decision edges */
  edges: DecisionEdge[];
  /** Entry node ID */
  startNodeId: string;
  /** Exit node ID */
  endNodeId?: string;
  /** Error node IDs (if any) */
  errorNodeIds?: string[];
  /** Total number of possible paths */
  totalPaths: number;
  /** Paths that were actually executed */
  executedPaths: number;
  /** Graph density (connectivity measure) */
  graphDensity?: number;
}

// ============================================================================
// Type Definitions: Execution Path
// ============================================================================

/**
 * Execution path step
 */
export interface ExecutionPathStep {
  /** Step sequence number */
  stepNo: number;
  /** Node ID in this step */
  nodeId: string;
  /** Node type */
  nodeType: "decision" | "action" | "tool_call" | "outcome";
  /** Node description */
  description: string;
  /** Iteration number */
  iteration: number;
  /** Timestamp of step */
  timestamp: number;
  /** Decision/action taken */
  action?: string;
  /** Result of this step */
  result?: unknown;
  /** Duration (ms) */
  duration?: number;
}

/**
 * Execution path (the actual path taken)
 */
export interface ExecutionPath {
  /** Unique path ID */
  pathId: string;
  /** Agent loop ID */
  agentLoopId: ID;
  /** All steps in the path */
  steps: ExecutionPathStep[];
  /** Whether path completed successfully */
  isSuccessful: boolean;
  /** Why execution ended */
  endReason?: string;
  /** Total execution time (ms) */
  totalDuration: number;
  /** Path complexity score */
  complexityScore?: number;
  /** Optimality score (0-1) */
  optimalityScore?: number;
}

// ============================================================================
// Type Definitions: Alternative Decisions
// ============================================================================

/**
 * Alternative decision option
 */
export interface AlternativeDecision {
  /** Option ID */
  optionId: string;
  /** Description of this alternative */
  description: string;
  /** Why this option was available */
  reason?: string;
  /** Estimated outcome */
  estimatedOutcome?: string;
  /** Probability this would succeed (0-1) */
  successProbability?: number;
  /** Cost/effort estimate */
  costEstimate?: number;
  /** Confidence in this estimate (0-1) */
  confidence?: number;
  /** Pros of this option */
  pros?: string[];
  /** Cons of this option */
  cons?: string[];
}

/**
 * Alternative decisions at a specific iteration
 */
export interface IterationAlternatives {
  /** Iteration number */
  iteration: number;
  /** Timestamp of decision point */
  timestamp: number;
  /** Node where decision was made */
  nodeId: string;
  /** Decision that was actually taken */
  chosenDecision: {
    description: string;
    reasoning?: string;
  };
  /** Other alternatives available */
  alternatives: AlternativeDecision[];
  /** Total alternatives evaluated */
  totalAlternatives: number;
}

// ============================================================================
// Type Definitions: Decision Sequence
// ============================================================================

/**
 * Single decision in the sequence
 */
export interface DecisionRecord {
  /** Sequence number */
  sequenceNo: number;
  /** Iteration number */
  iteration: number;
  /** Timestamp */
  timestamp: number;
  /** Decision description */
  description: string;
  /** Decision type */
  decisionType:
    | "tool_selection"
    | "parameter_choice"
    | "branching"
    | "iteration_control"
    | "output_format"
    | "error_handling";
  /** Reasoning for this decision */
  reasoning?: string;
  /** Alternatives considered */
  alternativesCount?: number;
  /** Confidence (0-1) */
  confidence?: number;
  /** Result of decision */
  result?: unknown;
}

/**
 * Complete decision sequence
 */
export interface DecisionSequence {
  /** Agent loop ID */
  agentLoopId: ID;
  /** Total decisions made */
  totalDecisions: number;
  /** All decisions in order */
  decisions: DecisionRecord[];
  /** Decision-making pattern analysis */
  patterns?: {
    mostCommonDecisionType: string;
    averageConfidence: number;
    decisionFrequency: Record<string, number>;
  };
}

// ============================================================================
// Query & Filter Types
// ============================================================================

/**
 * Decision graph filter
 */
export interface DecisionGraphFilter {
  /** Agent loop ID */
  agentLoopId?: ID;
  /** Min graph density */
  minDensity?: number;
  /** Max graph density */
  maxDensity?: number;
  /** Min iteration count */
  minIterations?: number;
}

/**
 * Execution path filter
 */
export interface ExecutionPathFilter {
  /** Agent loop ID */
  agentLoopId?: ID;
  /** Filter by success status */
  isSuccessful?: boolean;
  /** Min complexity */
  minComplexity?: number;
  /** Max complexity */
  maxComplexity?: number;
}

/**
 * Decision sequence filter
 */
export interface DecisionSequenceFilter {
  /** Agent loop ID */
  agentLoopId?: ID;
  /** Iteration range */
  iterationRange?: {
    start: number;
    end: number;
  };
  /** Decision type filter */
  decisionType?: string;
  /** Min confidence threshold */
  minConfidence?: number;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * AgentExecutionGraphQueryAPI - Agent Execution Graph Query API
 *
 * Provides queries for:
 * - Decision graph structure and topology
 * - Execution path analysis
 * - Alternative decisions at decision points
 * - Decision sequence and patterns
 */
export class AgentExecutionGraphQueryAPI extends QueryableResourceAPI<
  DecisionGraph,
  string,
  DecisionGraphFilter
> {
  private decisionGraphs: Map<string, DecisionGraph> = new Map();
  private executionPaths: Map<string, ExecutionPath> = new Map();
  private decisionSequences: Map<string, DecisionSequence> = new Map();
  private iterationAlternatives: Map<string, IterationAlternatives[]> = new Map();

  /**
   * Constructor
   * @param deps APIDependencyManager instance
   */
  constructor(deps: APIDependencyManager) {
    super();
    void deps; // Acknowledge parameter
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get decision graph by ID
   */
  protected override async getResource(id: string): Promise<DecisionGraph | null> {
    return this.decisionGraphs.get(id) ?? null;
  }

  /**
   * Get all decision graphs
   */
  protected override async getAllResources(): Promise<DecisionGraph[]> {
    return Array.from(this.decisionGraphs.values());
  }

  /**
   * Apply filters to decision graphs
   */
  protected override applyFilter(
    records: DecisionGraph[],
    filter: DecisionGraphFilter,
  ): DecisionGraph[] {
    let filtered = records;

    if (filter.agentLoopId) {
      filtered = filtered.filter(r => r.agentLoopId === filter.agentLoopId);
    }

    if (filter.minDensity !== undefined) {
      filtered = filtered.filter(r => (r.graphDensity ?? 0) >= filter.minDensity!);
    }

    if (filter.maxDensity !== undefined) {
      filtered = filtered.filter(r => (r.graphDensity ?? 1) <= filter.maxDensity!);
    }

    if (filter.minIterations !== undefined) {
      filtered = filtered.filter(r => r.nodes.length >= filter.minIterations!);
    }

    return filtered;
  }

  // ============================================================================
  // Decision Graph Queries
  // ============================================================================

  /**
   * Get decision graph
   *
   * @param agentLoopId Agent loop ID
   * @returns Complete decision graph
   */
  async getDecisionGraph(agentLoopId: ID): Promise<DecisionGraph | null> {
    return this.decisionGraphs.get(agentLoopId as string) ?? null;
  }

  /**
   * Get decision nodes
   *
   * @param agentLoopId Agent loop ID
   * @returns All decision nodes
   */
  async getDecisionNodes(agentLoopId: ID): Promise<DecisionNode[]> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    return graph?.nodes ?? [];
  }

  /**
   * Get decision edges
   *
   * @param agentLoopId Agent loop ID
   * @returns All decision edges
   */
  async getDecisionEdges(agentLoopId: ID): Promise<DecisionEdge[]> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    return graph?.edges ?? [];
  }

  /**
   * Get outgoing edges from a node
   *
   * @param agentLoopId Agent loop ID
   * @param nodeId Source node ID
   * @returns Edges from this node
   */
  async getOutgoingEdges(agentLoopId: ID, nodeId: string): Promise<DecisionEdge[]> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    if (!graph) return [];

    return graph.edges.filter(e => e.fromNodeId === nodeId);
  }

  /**
   * Get incoming edges to a node
   *
   * @param agentLoopId Agent loop ID
   * @param nodeId Target node ID
   * @returns Edges to this node
   */
  async getIncomingEdges(agentLoopId: ID, nodeId: string): Promise<DecisionEdge[]> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    if (!graph) return [];

    return graph.edges.filter(e => e.toNodeId === nodeId);
  }

  /**
   * Get all paths in the graph
   *
   * @param agentLoopId Agent loop ID
   * @returns All possible paths
   */
  async getAllPaths(agentLoopId: ID): Promise<string[][]> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    if (!graph) return [];

    // Find all paths from start to end nodes
    const paths: string[][] = [];
    const visited = new Set<string>();
    const currentPath: string[] = [];

    const dfs = (nodeId: string) => {
      visited.add(nodeId);
      currentPath.push(nodeId);

      if (nodeId === graph.endNodeId || graph.errorNodeIds?.includes(nodeId)) {
        paths.push([...currentPath]);
      } else {
        const outgoing = graph.edges.filter(e => e.fromNodeId === nodeId);
        for (const edge of outgoing) {
          if (!visited.has(edge.toNodeId)) {
            dfs(edge.toNodeId);
          }
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
   * Get execution path
   *
   * @param agentLoopId Agent loop ID
   * @returns The actual execution path taken
   */
  async getExecutionPath(agentLoopId: ID): Promise<ExecutionPath | null> {
    return this.executionPaths.get(agentLoopId as string) ?? null;
  }

  /**
   * Get execution path steps
   *
   * @param agentLoopId Agent loop ID
   * @returns Steps in execution path
   */
  async getExecutionPathSteps(agentLoopId: ID): Promise<ExecutionPathStep[]> {
    const path = this.executionPaths.get(agentLoopId as string);
    return path?.steps ?? [];
  }

  /**
   * Get path statistics
   *
   * @param agentLoopId Agent loop ID
   * @returns Path analysis statistics
   */
  async getPathStatistics(
    agentLoopId: ID,
  ): Promise<{
    stepsCount: number;
    totalDuration: number;
    averageIterationDuration: number;
    complexityScore: number;
    optimalityScore: number;
  } | null> {
    const path = this.executionPaths.get(agentLoopId as string);
    if (!path) return null;

    const avgIterationDuration =
      path.steps.length > 0 ? path.totalDuration / path.steps.length : 0;

    return {
      stepsCount: path.steps.length,
      totalDuration: path.totalDuration,
      averageIterationDuration: avgIterationDuration,
      complexityScore: path.complexityScore ?? 0,
      optimalityScore: path.optimalityScore ?? 0,
    };
  }

  // ============================================================================
  // Alternative Decisions Queries
  // ============================================================================

  /**
   * Get alternative decisions at a specific iteration
   *
   * @param agentLoopId Agent loop ID
   * @param iterationNo Iteration number
   * @returns Alternatives available at this iteration
   */
  async getAlternativeDecisions(
    agentLoopId: ID,
    iterationNo: number,
  ): Promise<IterationAlternatives | null> {
    const alternatives = this.iterationAlternatives.get(agentLoopId as string) ?? [];
    return alternatives.find(a => a.iteration === iterationNo) ?? null;
  }

  /**
   * Get all decision alternatives
   *
   * @param agentLoopId Agent loop ID
   * @returns All alternatives at all decision points
   */
  async getAllAlternatives(agentLoopId: ID): Promise<IterationAlternatives[]> {
    return this.iterationAlternatives.get(agentLoopId as string) ?? [];
  }

  /**
   * Get alternatives not taken (counterfactual analysis)
   *
   * @param agentLoopId Agent loop ID
   * @returns Alternatives that were not chosen
   */
  async getUnexploredAlternatives(agentLoopId: ID): Promise<AlternativeDecision[]> {
    const alternatives = this.iterationAlternatives.get(agentLoopId as string) ?? [];
    const unexplored: AlternativeDecision[] = [];

    for (const iteration of alternatives) {
      unexplored.push(...iteration.alternatives);
    }

    return unexplored;
  }

  /**
   * Get decision with highest success probability not taken
   *
   * @param agentLoopId Agent loop ID
   * @returns Most promising alternative not taken
   */
  async getMostPromisingUnexplored(agentLoopId: ID): Promise<AlternativeDecision | null> {
    const unexplored = await this.getUnexploredAlternatives(agentLoopId);
    if (unexplored.length === 0) return null;

    return unexplored.reduce((best, current) => {
      const currentProb = current.successProbability ?? 0;
      const bestProb = best.successProbability ?? 0;
      return currentProb > bestProb ? current : best;
    });
  }

  // ============================================================================
  // Decision Sequence Queries
  // ============================================================================

  /**
   * Get decision sequence
   *
   * @param agentLoopId Agent loop ID
   * @returns Complete decision sequence
   */
  async getDecisionSequence(agentLoopId: ID): Promise<DecisionSequence | null> {
    return this.decisionSequences.get(agentLoopId as string) ?? null;
  }

  /**
   * Get decisions at specific iteration
   *
   * @param agentLoopId Agent loop ID
   * @param iteration Iteration number
   * @returns Decisions made in this iteration
   */
  async getDecisionsInIteration(agentLoopId: ID, iteration: number): Promise<DecisionRecord[]> {
    const sequence = this.decisionSequences.get(agentLoopId as string);
    if (!sequence) return [];

    return sequence.decisions.filter(d => d.iteration === iteration);
  }

  /**
   * Get decision by type
   *
   * @param agentLoopId Agent loop ID
   * @param decisionType Type of decision
   * @returns Decisions of this type
   */
  async getDecisionsByType(
    agentLoopId: ID,
    decisionType: string,
  ): Promise<DecisionRecord[]> {
    const sequence = this.decisionSequences.get(agentLoopId as string);
    if (!sequence) return [];

    return sequence.decisions.filter(d => d.decisionType === decisionType);
  }

  /**
   * Get high-confidence decisions
   *
   * @param agentLoopId Agent loop ID
   * @param threshold Confidence threshold (0-1)
   * @returns Decisions above threshold
   */
  async getHighConfidenceDecisions(
    agentLoopId: ID,
    threshold: number = 0.8,
  ): Promise<DecisionRecord[]> {
    const sequence = this.decisionSequences.get(agentLoopId as string);
    if (!sequence) return [];

    return sequence.decisions.filter(d => (d.confidence ?? 0) >= threshold);
  }

  /**
   * Get low-confidence decisions
   *
   * @param agentLoopId Agent loop ID
   * @param threshold Confidence threshold (0-1)
   * @returns Decisions below threshold
   */
  async getLowConfidenceDecisions(
    agentLoopId: ID,
    threshold: number = 0.5,
  ): Promise<DecisionRecord[]> {
    const sequence = this.decisionSequences.get(agentLoopId as string);
    if (!sequence) return [];

    return sequence.decisions.filter(d => (d.confidence ?? 1) < threshold);
  }

  /**
   * Analyze decision patterns
   *
   * @param agentLoopId Agent loop ID
   * @returns Pattern analysis
   */
  async analyzeDecisionPatterns(
    agentLoopId: ID,
  ): Promise<{
    dominantDecisionType: string;
    averageConfidence: number;
    decisionFrequency: Record<string, number>;
    consistencyScore: number;
  } | null> {
    const sequence = this.decisionSequences.get(agentLoopId as string);
    if (!sequence || !sequence.patterns) return null;

    // Calculate consistency based on confidence variance
    const confidences = sequence.decisions.map(d => d.confidence ?? 0.5);
    const avgConfidence = confidences.reduce((a, b) => a + b, 0) / confidences.length;
    const variance =
      confidences.reduce((sum, conf) => sum + Math.pow(conf - avgConfidence, 2), 0) /
      confidences.length;
    const consistencyScore = Math.max(0, 1 - Math.sqrt(variance));

    return {
      dominantDecisionType: sequence.patterns.mostCommonDecisionType,
      averageConfidence: sequence.patterns.averageConfidence,
      decisionFrequency: sequence.patterns.decisionFrequency,
      consistencyScore,
    };
  }

  // ============================================================================
  // Combined Graph & Path Analysis
  // ============================================================================

  /**
   * Analyze efficiency: how well the actual path matches the optimal path
   *
   * @param agentLoopId Agent loop ID
   * @returns Efficiency analysis
   */
  async analyzePathEfficiency(
    agentLoopId: ID,
  ): Promise<{
    executedSteps: number;
    optimalSteps: number;
    efficiencyRatio: number;
    wastefulDecisions: number;
  } | null> {
    const path = this.executionPaths.get(agentLoopId as string);
    const graph = this.decisionGraphs.get(agentLoopId as string);

    if (!path || !graph) return null;

    // Find shortest path in graph
    const allPaths = await this.getAllPaths(agentLoopId);
    const shortestPathLength = Math.min(...allPaths.map(p => p.length), Infinity);

    return {
      executedSteps: path.steps.length,
      optimalSteps: shortestPathLength === Infinity ? path.steps.length : shortestPathLength,
      efficiencyRatio:
        shortestPathLength === Infinity ? 1 : path.steps.length / shortestPathLength,
      wastefulDecisions: Math.max(0, path.steps.length - shortestPathLength),
    };
  }

  /**
   * Get the critical path (longest path) through the decision graph.
   * Based on the number of nodes in each path since DecisionNode does not have duration.
   *
   * @param agentLoopId Agent loop ID
   * @returns Critical path node IDs or null
   */
  async getCriticalPath(agentLoopId: ID): Promise<string[] | null> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    if (!graph) return null;

    const allPaths = await this.getAllPaths(agentLoopId);
    if (allPaths.length === 0) return null;

    // Return the longest path by node count
    let criticalPath: string[] = [];
    let maxLength = 0;

    for (const path of allPaths) {
      if (path.length > maxLength) {
        maxLength = path.length;
        criticalPath = path;
      }
    }

    return criticalPath;
  }

  /**
   * Analyze path probabilities across the decision graph.
   *
   * @param agentLoopId Agent loop ID
   * @returns Path probability analysis or null
   */
  async getPathProbabilityAnalysis(
    agentLoopId: ID,
  ): Promise<{
    agentLoopId: ID;
    paths: Array<{
      pathId: string;
      nodeIds: string[];
      probability: number;
      isTaken: boolean;
    }>;
    mostLikelyPath: string[] | null;
    pathDiversity: number;
  } | null> {
    const graph = this.decisionGraphs.get(agentLoopId as string);
    if (!graph) return null;

    const allPaths = await this.getAllPaths(agentLoopId);
    const path = this.executionPaths.get(agentLoopId as string);

    const paths = allPaths.map((nodeIds, index) => {
      let probability = 1;
      for (let i = 0; i < nodeIds.length - 1; i++) {
        const edge = graph.edges.find(
          (e) => e.fromNodeId === nodeIds[i] && e.toNodeId === nodeIds[i + 1],
        );
        if (edge?.probability !== undefined) {
          probability *= edge.probability;
        }
      }

      const isTaken = path
        ? nodeIds.every((id) => path.steps.some((s) => s.nodeId === id))
        : false;

      return {
        pathId: `path-${index}`,
        nodeIds,
        probability,
        isTaken,
      };
    });

    paths.sort((a, b) => b.probability - a.probability);

    const mostLikelyPath = paths[0]?.nodeIds ?? null;
    const totalProb = paths.reduce((sum, p) => sum + p.probability, 0);
    const pathDiversity =
      totalProb > 0
        ? -paths.reduce((sum, p) => {
            const normalizedP = p.probability / totalProb;
            return sum + (normalizedP > 0 ? normalizedP * Math.log2(normalizedP) : 0);
          }, 0) / Math.log2(paths.length)
        : 0;

    return {
      agentLoopId,
      paths,
      mostLikelyPath,
      pathDiversity: Math.round(pathDiversity * 100) / 100,
    };
  }
}
