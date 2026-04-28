/**
 * GraphData Structure
 *
 * Design Notes:
 * - GraphData is an implementation class of the Graph interface.
 * - It provides basic data storage and querying functions for graphs.
 * - As a core entity, it is located in the core/entities directory.
 *
 * Core Responsibilities:
 * - Stores graph nodes, edges, and their adjacency relationships.
 * - Provides methods for querying and traversing the graph.
 * - Acts as a stateless data structure, without any state management logic.
 *
 * Use Cases:
 * - Representation of graph structures defined in workflows.
 * - Graph data during Thread execution (immutable).
 * - Graph validation and analysis.
 *
 * Precautions:
 * - GraphData is a stateless data structure; once constructed, it should not be modified.
 * - The construction of the graph is handled by the GraphBuilder.
 * - At runtime, it is managed by the GraphRegistry to ensure immutability.
 */

import type {
  GraphNode,
  GraphEdge,
  AdjacencyList,
  ReverseAdjacencyList,
  NodeMap,
  EdgeMap,
  Graph,
} from "@wf-agent/types";
import type { ID } from "@wf-agent/types";

/**
 * Graph Data Structure Class
 * Core Responsibilities: Store and manage the nodes, edges, and adjacency relationships of a graph.
 * Does not include complex algorithms; only provides basic graph operations.
 */
export class GraphData implements Graph {
  /** Node set */
  public nodes: NodeMap;
  /** Edge Set */
  public edges: EdgeMap;
  /** Forward Adjacency List */
  public adjacencyList: AdjacencyList;
  /** Reverse Adjacency List */
  public reverseAdjacencyList: ReverseAdjacencyList;
  /** Starting node ID */
  public startNodeId?: ID;
  /** End node ID set */
  public endNodeIds: Set<ID>;

  constructor() {
    this.nodes = new Map();
    this.edges = new Map();
    this.adjacencyList = new Map();
    this.reverseAdjacencyList = new Map();
    this.endNodeIds = new Set();
  }

  /**
   * Add a node
   * Note: This method is only for the construction phase and should not be called at runtime.
   */
  addNode(node: GraphNode): void {
    this.nodes.set(node.id, node);
    // Initialize the adjacency list
    if (!this.adjacencyList.has(node.id)) {
      this.adjacencyList.set(node.id, new Set());
    }
    if (!this.reverseAdjacencyList.has(node.id)) {
      this.reverseAdjacencyList.set(node.id, new Set());
    }
  }

  /**
   * Add edges
   * Note: This method is only for the construction phase and should not be called at runtime.
   */
  addEdge(edge: GraphEdge): void {
    this.edges.set(edge.id, edge);
    // Update the forward adjacency list
    if (!this.adjacencyList.has(edge.sourceNodeId)) {
      this.adjacencyList.set(edge.sourceNodeId, new Set());
    }
    this.adjacencyList.get(edge.sourceNodeId)!.add(edge.targetNodeId);

    // Update the reverse adjacency list
    if (!this.reverseAdjacencyList.has(edge.targetNodeId)) {
      this.reverseAdjacencyList.set(edge.targetNodeId, new Set());
    }
    this.reverseAdjacencyList.get(edge.targetNodeId)!.add(edge.sourceNodeId);
  }

  /**
   * Get the node
   */
  getNode(nodeId: ID): GraphNode | undefined {
    return this.nodes.get(nodeId);
  }

  /**
   * Get the edges
   */
  getEdge(edgeId: ID): GraphEdge | undefined {
    return this.edges.get(edgeId);
  }

  /**
   * Get the out-degree neighbors of a node
   */
  getOutgoingNeighbors(nodeId: ID): Set<ID> {
    return this.adjacencyList.get(nodeId) || new Set();
  }

  /**
   * Get the in-degree neighbors of a node.
   */
  getIncomingNeighbors(nodeId: ID): Set<ID> {
    return this.reverseAdjacencyList.get(nodeId) || new Set();
  }

  /**
   * Get the outgoing edges of a node
   */
  getOutgoingEdges(nodeId: ID): GraphEdge[] {
    const neighbors = this.getOutgoingNeighbors(nodeId);
    const edges: GraphEdge[] = [];
    for (const edge of this.edges.values()) {
      if (edge.sourceNodeId === nodeId && neighbors.has(edge.targetNodeId)) {
        edges.push(edge);
      }
    }
    return edges;
  }

  /**
   * Get the in-degree of a node
   */
  getIncomingEdges(nodeId: ID): GraphEdge[] {
    const neighbors = this.getIncomingNeighbors(nodeId);
    const edges: GraphEdge[] = [];
    for (const edge of this.edges.values()) {
      if (edge.targetNodeId === nodeId && neighbors.has(edge.sourceNodeId)) {
        edges.push(edge);
      }
    }
    return edges;
  }

  /**
   * Get the edge between two nodes
   */
  getEdgeBetween(sourceNodeId: ID, targetNodeId: ID): GraphEdge | undefined {
    for (const edge of this.edges.values()) {
      if (edge.sourceNodeId === sourceNodeId && edge.targetNodeId === targetNodeId) {
        return edge;
      }
    }
    return undefined;
  }

  /**
   * Check if the node exists.
   */
  hasNode(nodeId: ID): boolean {
    return this.nodes.has(nodeId);
  }

  /**
   * Check if the edge exists.
   */
  hasEdge(edgeId: ID): boolean {
    return this.edges.has(edgeId);
  }

  /**
   * Check if there is an edge between two nodes.
   */
  hasEdgeBetween(sourceNodeId: ID, targetNodeId: ID): boolean {
    const neighbors = this.adjacencyList.get(sourceNodeId);
    return neighbors ? neighbors.has(targetNodeId) : false;
  }

  /**
   * Get all node IDs
   */
  getAllNodeIds(): ID[] {
    return Array.from(this.nodes.keys());
  }

  /**
   * Get all edge IDs
   */
  getAllEdgeIds(): ID[] {
    return Array.from(this.edges.keys());
  }

  /**
   * Get the number of nodes.
   */
  getNodeCount(): number {
    return this.nodes.size;
  }

  /**
   * Get the number of edges
   */
  getEdgeCount(): number {
    return this.edges.size;
  }

  /**
   * Get the node with an in-degree of 0.
   */
  getSourceNodes(): GraphNode[] {
    const sources: GraphNode[] = [];
    for (const [nodeId, neighbors] of this.reverseAdjacencyList) {
      if (neighbors.size === 0) {
        const node = this.nodes.get(nodeId);
        if (node) {
          sources.push(node);
        }
      }
    }
    return sources;
  }

  /**
   * Get the nodes with an outdegree of 0.
   */
  getSinkNodes(): GraphNode[] {
    const sinks: GraphNode[] = [];
    for (const [nodeId, neighbors] of this.adjacencyList) {
      if (neighbors.size === 0) {
        const node = this.nodes.get(nodeId);
        if (node) {
          sinks.push(node);
        }
      }
    }
    return sinks;
  }

  /**
   * Clear the image.
   */
  clear(): void {
    this.nodes.clear();
    this.edges.clear();
    this.adjacencyList.clear();
    this.reverseAdjacencyList.clear();
    this.endNodeIds.clear();
    this.startNodeId = undefined;
  }
}
