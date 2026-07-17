/**
 * Workflow Graph Structure
 *
 * Design Notes:
 * - WorkflowGraphStructure represents the core graph topology
 * - Immutable after construction for thread safety
 * - All preprocessing data is separated into WorkflowGraphMetadata
 *
 * Core Responsibilities:
 * - Stores graph nodes, edges, and their adjacency relationships.
 * - Provides methods for querying and traversing the graph.
 * - Acts as a stateless data structure without any state management logic.
 *
 * Usage:
 * - Representation of graph structures defined in workflows.
 * - Graph data during WorkflowExecution execution (immutable).
 * - Graph validation and analysis.
 *
 * Design Principles:
 * - WorkflowGraphStructure is immutable once constructed.
 * - Construction is handled by the WorkflowGraphBuilder.
 * - At runtime, managed by WorkflowGraphRegistry to ensure immutability.
 * - Preprocessing data is stored separately in WorkflowGraphMetadata.
 */

import type {
  WorkflowNode,
  WorkflowEdge,
  AdjacencyList,
  ReverseAdjacencyList,
  NodeMap,
  EdgeMap,
} from "@wf-agent/types";
import type { ID } from "@wf-agent/types";

/**
 * Workflow Graph Structure Class
 * Core Responsibilities: Store and manage the nodes, edges, and adjacency relationships of a graph.
 * Does not include complex algorithms or preprocessing data; only provides basic graph operations.
 */
export class WorkflowGraphStructureImpl {
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
  /**
   * Outgoing edges indexed by source node ID (built during construction for O(degree) lookup).
   *
   * SYNC REQUIREMENT: When addEdge() is called, this index is updated alongside edges,
   * adjacencyList, and reverseAdjacencyList. If edge removal is ever added, ALL four
   * structures must be kept in sync.
   */
  public _outgoingEdgeMap: Map<ID, WorkflowEdge[]>;
  /**
   * Incoming edges indexed by target node ID (built during construction for O(degree) lookup).
   *
   * SYNC REQUIREMENT: Same as _outgoingEdgeMap — must be kept in sync with all other
   * edge-related structures whenever edges are added or removed.
   */
  public _incomingEdgeMap: Map<ID, WorkflowEdge[]>;

  constructor() {
    this.nodes = new Map();
    this.edges = new Map();
    this.adjacencyList = new Map();
    this.reverseAdjacencyList = new Map();
    this.endNodeIds = new Set();
    this._outgoingEdgeMap = new Map();
    this._incomingEdgeMap = new Map();
  }

  /**
   * Add a node
   * Note: This method is only for the construction phase and should not be called at runtime.
   */
  addNode(node: WorkflowNode): void {
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
  addEdge(edge: WorkflowEdge): void {
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

    // Update outgoing edge index
    if (!this._outgoingEdgeMap.has(edge.sourceNodeId)) {
      this._outgoingEdgeMap.set(edge.sourceNodeId, []);
    }
    this._outgoingEdgeMap.get(edge.sourceNodeId)!.push(edge);

    // Update incoming edge index
    if (!this._incomingEdgeMap.has(edge.targetNodeId)) {
      this._incomingEdgeMap.set(edge.targetNodeId, []);
    }
    this._incomingEdgeMap.get(edge.targetNodeId)!.push(edge);
  }

  /**
   * Get the node
   */
  getNode(nodeId: ID): WorkflowNode | undefined {
    return this.nodes.get(nodeId);
  }

  /**
   * Get the edges
   */
  getEdge(edgeId: ID): WorkflowEdge | undefined {
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
   * Uses the pre-built outgoing edge index for O(degree) performance.
   */
  getOutgoingEdges(nodeId: ID): WorkflowEdge[] {
    return this._outgoingEdgeMap.get(nodeId) || [];
  }

  /**
   * Get the incoming edges of a node
   * Uses the pre-built incoming edge index for O(degree) performance.
   */
  getIncomingEdges(nodeId: ID): WorkflowEdge[] {
    return this._incomingEdgeMap.get(nodeId) || [];
  }

  /**
   * Get the edge between two nodes
   * Uses the outgoing edge index for O(degree) performance.
   */
  getEdgeBetween(sourceNodeId: ID, targetNodeId: ID): WorkflowEdge | undefined {
    const outgoing = this._outgoingEdgeMap.get(sourceNodeId);
    if (!outgoing) return undefined;
    return outgoing.find(edge => edge.targetNodeId === targetNodeId);
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
   * Get the number of nodes
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
   * Get source nodes (nodes with in-degree 0)
   */
  getSourceNodes(): WorkflowNode[] {
    const sources: WorkflowNode[] = [];
    for (const node of this.nodes.values()) {
      const incoming = this.getIncomingNeighbors(node.id);
      if (incoming.size === 0) {
        sources.push(node);
      }
    }
    return sources;
  }

  /**
   * Get sink nodes (nodes with out-degree 0)
   */
  getSinkNodes(): WorkflowNode[] {
    const sinks: WorkflowNode[] = [];
    for (const node of this.nodes.values()) {
      const outgoing = this.getOutgoingNeighbors(node.id);
      if (outgoing.size === 0) {
        sinks.push(node);
      }
    }
    return sinks;
  }
}