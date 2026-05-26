/**
 * WorkflowGraphQueryAPI - Workflow Graph Structure Query API
 * Provides query capabilities for the pre-processed workflow graph structure
 *
 * Responsibilities:
 * - Query full workflow graph by workflow ID
 * - Query nodes, edges, adjacency information
 * - Query graph analysis results (cycle detection, topology, etc.)
 * - Query graph statistics (node/edge counts by type)
 * - Wraps WorkflowGraphRegistry from the SDK core layer
 */

import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type {
  WorkflowNode,
  WorkflowEdge,
} from "@wf-agent/types";
import type { ID } from "@wf-agent/types";
import type { WorkflowGraph } from "../../../../workflow/types/graph/preprocessed-graph.js";
import type { WorkflowGraphAnalysis } from "../../../../workflow/types/graph/analysis.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "WorkflowGraphQueryAPI" });

/**
 * Node statistics summary
 */
export interface GraphNodeStats {
  /** Total number of nodes */
  total: number;
  /** Number of nodes grouped by type */
  byType: Record<string, number>;
}

/**
 * Edge statistics summary
 */
export interface GraphEdgeStats {
  /** Total number of edges */
  total: number;
  /** Number of edges grouped by type */
  byType: Record<string, number>;
}

/**
 * Workflow graph summary (lightweight)
 */
export interface WorkflowGraphSummary {
  /** Workflow ID */
  workflowId: ID;
  /** Total nodes */
  nodeCount: number;
  /** Total edges */
  edgeCount: number;
  /** Node statistics */
  nodeStats: GraphNodeStats;
  /** Edge statistics */
  edgeStats: GraphEdgeStats;
  /** Whether analysis results exist */
  hasAnalysis: boolean;
}

/**
 * Node neighbor query result
 */
export interface NodeNeighbors {
  /** Node ID */
  nodeId: ID;
  /** Incoming neighbor node IDs */
  predecessors: ID[];
  /** Outgoing neighbor node IDs */
  successors: ID[];
}

/**
 * WorkflowGraphQueryAPI - Workflow Graph Structure Query API
 */
export class WorkflowGraphQueryAPI {
  private graphRegistry: import("../../../../workflow/stores/workflow-graph-registry.js").WorkflowGraphRegistry;

  constructor(deps: APIDependencyManager) {
    this.graphRegistry = deps.getWorkflowGraphRegistry();
    logger.info("WorkflowGraphQueryAPI initialized");
  }

  /**
   * Get the full workflow graph by workflow ID
   * @param workflowId Workflow ID
   * @returns The full WorkflowGraph or undefined if not found
   */
  async getGraph(workflowId: ID): Promise<WorkflowGraph | undefined> {
    return this.graphRegistry.get(workflowId);
  }

  /**
   * Check if a workflow graph exists
   * @param workflowId Workflow ID
   * @returns Whether the graph exists
   */
  async hasGraph(workflowId: ID): Promise<boolean> {
    return this.graphRegistry.has(workflowId);
  }

  /**
   * Get graph summary (lightweight stats without full graph data)
   * @param workflowId Workflow ID
   * @returns Graph summary or null if not found
   */
  async getGraphSummary(workflowId: ID): Promise<WorkflowGraphSummary | null> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph) return null;

    const nodeStats = this.computeNodeStats(graph);
    const edgeStats = this.computeEdgeStats(graph);

    return {
      workflowId: graph.workflowId,
      nodeCount: nodeStats.total,
      edgeCount: edgeStats.total,
      nodeStats,
      edgeStats,
      hasAnalysis: !!graph.graphAnalysis,
    };
  }

  /**
   * Get all nodes in the workflow graph
   * @param workflowId Workflow ID
   * @returns Array of nodes or empty array if not found
   */
  async getNodes(workflowId: ID): Promise<WorkflowNode[]> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph) return [];
    const nodes: WorkflowNode[] = [];
    for (const node of graph.nodes.values()) {
      nodes.push(node);
    }
    return nodes;
  }

  /**
   * Get nodes filtered by type
   * @param workflowId Workflow ID
   * @param nodeType Node type to filter by (e.g., "START", "LLM", "SCRIPT")
   * @returns Array of matching nodes
   */
  async getNodesByType(workflowId: ID, nodeType: string): Promise<WorkflowNode[]> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph) return [];
    const nodes: WorkflowNode[] = [];
    for (const node of graph.nodes.values()) {
      if (node.type === nodeType) {
        nodes.push(node);
      }
    }
    return nodes;
  }

  /**
   * Get all edges in the workflow graph
   * @param workflowId Workflow ID
   * @returns Array of edges or empty array if not found
   */
  async getEdges(workflowId: ID): Promise<WorkflowEdge[]> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph) return [];
    const edges: WorkflowEdge[] = [];
    for (const edge of graph.edges.values()) {
      edges.push(edge);
    }
    return edges;
  }

  /**
   * Get neighbors (predecessors and successors) for a specific node
   * @param workflowId Workflow ID
   * @param nodeId Node ID
   * @returns Node neighbor information or null if node not found
   */
  async getNodeNeighbors(workflowId: ID, nodeId: ID): Promise<NodeNeighbors | null> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph) return null;

    const predecessors: ID[] = [];
    const successors: ID[] = [];

    // Check adjacency list for successors
    const adjSet = graph.adjacencyList?.get(nodeId);
    if (adjSet) {
      successors.push(...adjSet);
    }

    // Check reverse adjacency list for predecessors
    const revSet = graph.reverseAdjacencyList?.get(nodeId);
    if (revSet) {
      predecessors.push(...revSet);
    }

    return { nodeId, predecessors, successors };
  }

  /**
   * Get graph analysis results
   * @param workflowId Workflow ID
   * @returns Graph analysis or null if not found or analysis not computed
   */
  async getGraphAnalysis(workflowId: ID): Promise<WorkflowGraphAnalysis | null> {
    const graph = this.graphRegistry.get(workflowId);
    if (!graph?.graphAnalysis) return null;
    return graph.graphAnalysis;
  }

  /**
   * Compute node statistics from the graph
   */
  private computeNodeStats(graph: WorkflowGraph): GraphNodeStats {
    const byType: Record<string, number> = {};
    for (const node of graph.nodes.values()) {
      byType[node.type] = (byType[node.type] || 0) + 1;
    }
    return {
      total: graph.nodes.size,
      byType,
    };
  }

  /**
   * Compute edge statistics from the graph
   */
  private computeEdgeStats(graph: WorkflowGraph): GraphEdgeStats {
    const byType: Record<string, number> = {};
    for (const edge of graph.edges.values()) {
      byType[edge.type] = (byType[edge.type] || 0) + 1;
    }
    return {
      total: graph.edges.size,
      byType,
    };
  }

  /**
   * List all workflow IDs that have registered graphs
   * @returns Array of workflow IDs
   */
  async listGraphWorkflows(): Promise<ID[]> {
    return this.graphRegistry.getAllWorkflowIds();
  }
}