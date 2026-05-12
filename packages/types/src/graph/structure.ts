/**
 * Graph Structure Type Definition
 */

import type { ID } from "../common.js";
import type { StaticNode, RuntimeNode } from "../node/index.js";
import type { Edge, EdgeType } from "../edge.js";

/**
 * Workflow Node Type
 * 
 * Represents nodes in the workflow execution graph.
 * Extends RuntimeNode with additional metadata for graph operations.
 * 
 * This is used during:
 * - Graph building and preprocessing
 * - Graph validation and analysis  
 * - Workflow execution (nodes retrieved from the graph)
 */
export interface WorkflowNode extends Omit<RuntimeNode, 'name'> {
  /** Node name (copied from static node for logging/debugging) */
  name: string;
  
  /** Optional node description (copied from static node) */
  description?: string;
  
  /** Reference to the original static node definition */
  originalNode?: StaticNode;
}

/**
 * Workflow Edge Types
 * Edge representations for graph validation and analysis
 */
export interface WorkflowEdge {
  /** edge unique identifier */
  id: ID;
  /** Source node ID */
  sourceNodeId: ID;
  /** Target Node ID */
  targetNodeId: ID;
  /** side type */
  type: EdgeType;
  /** Optional side labels */
  label?: string;
  /** Optional side descriptions */
  description?: string;
  /** Edge weights for sorting when multiple conditional edges are satisfied simultaneously */
  weight?: number;
  /** Raw edge reference (for accessing the full edge configuration) */
  originalEdge?: Edge;
}

/**
 * Neighbor-joining table type
 * Record the list of outgoing neighbor nodes for each node
 */
export type AdjacencyList = Map<ID, Set<ID>>;

/**
 * Reverse neighbor table type
 * Record the list of in-edge neighbor nodes for each node
 */
export type ReverseAdjacencyList = Map<ID, Set<ID>>;

/**
 * Node Mapping Table Types
 * Creates a mapping of node IDs to node objects
 */
export type NodeMap = Map<ID, WorkflowNode>;

/**
 * Edge Mapping Table Types
 * Creates a mapping relationship from edge IDs to edge objects
 */
export type EdgeMap = Map<ID, WorkflowEdge>;

/**
 * Workflow Graph Structure Interface
 *
 * Design Notes:
 * - Defines the interface specification for graph data structures
 * - The GraphData class implements this interface at the core level.
 * - WorkflowExecution and other types use this interface to reference graph data.
 * - Provide basic operations and query functions for graphs
 */
export interface WorkflowGraphStructure {
  /** collection of nodes */
  nodes: NodeMap;
  /** bilateral assembly */
  edges: EdgeMap;
  /** Forward adjacency table: record the outgoing edge neighbors of each node */
  adjacencyList: AdjacencyList;
  /** Reverse adjacency table: record the incoming neighbors of each node */
  reverseAdjacencyList: ReverseAdjacencyList;
  /** Starting node ID */
  startNodeId?: ID;
  /** Collection of end node IDs (there may be more than one END node) */
  endNodeIds: Set<ID>;

  /** Get node */
  getNode(nodeId: ID): WorkflowNode | undefined;
  /** Getting the edge */
  getEdge(edgeId: ID): WorkflowEdge | undefined;
  /** Get the outgoing neighbors of the node */
  getOutgoingNeighbors(nodeId: ID): Set<ID>;
  /** Get the incoming edge neighbors of the node */
  getIncomingNeighbors(nodeId: ID): Set<ID>;
  /** Get the outgoing edge of the node */
  getOutgoingEdges(nodeId: ID): WorkflowEdge[];
  /** Get the incoming edge of the node */
  getIncomingEdges(nodeId: ID): WorkflowEdge[];
  /** Get the edge between two nodes */
  getEdgeBetween(sourceNodeId: ID, targetNodeId: ID): WorkflowEdge | undefined;
  /** Check if the node exists */
  hasNode(nodeId: ID): boolean;
  /** Check for the presence of edges */
  hasEdge(edgeId: ID): boolean;
  /** Check if there is an edge between two nodes */
  hasEdgeBetween(sourceNodeId: ID, targetNodeId: ID): boolean;
  /** Get all node IDs */
  getAllNodeIds(): ID[];
  /** Get all edge IDs */
  getAllEdgeIds(): ID[];
  /** Get the number of nodes */
  getNodeCount(): number;
  /** Get the number of edges */
  getEdgeCount(): number;
  /** Get the node with in-degree 0 */
  getSourceNodes(): WorkflowNode[];
  /** Get the node with out degree 0 */
  getSinkNodes(): WorkflowNode[];
}
