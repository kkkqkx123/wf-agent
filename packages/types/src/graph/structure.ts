/**
 * Graph Structure Type Definition
 */

import type { ID, Metadata } from "../common.js";
import type { Node } from "../node/index.js";
import { NodeType } from "../node/index.js";
import type { Edge, EdgeType } from "../edge.js";

/**
 * Graph Node Types
 * Node representations for graph validation and analysis
 */
export interface GraphNode {
  /** Node Unique Identifier */
  id: ID;
  /** Node type */
  type: NodeType;
  /** Node Name */
  name: string;
  /** Optional node description */
  description?: string;
  /** Internal metadata (used internally by the system, not set by the user, to avoid data injection vulnerability) [does not exist in the node definition, only in the graph stage] */
  internalMetadata?: Metadata;
  /** Original node reference (for accessing the full node configuration) */
  originalNode?: Node;
  /** The original workflow ID to which the node belongs */
  workflowId: ID;
  /** Parent workflow ID (if it is a node of a subgraph expansion) */
  parentWorkflowId?: ID;
}

/**
 * Graph Edge Types
 * Edge representations for graph validation and analysis
 */
export interface GraphEdge {
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
export type NodeMap = Map<ID, GraphNode>;

/**
 * Edge Mapping Table Types
 * Creates a mapping relationship from edge IDs to edge objects
 */
export type EdgeMap = Map<ID, GraphEdge>;

/**
 * Graph Data Interface (Graph)
 *
 * Design Notes:
 * - Defines the interface specification for graph data structures
 * - The GraphData class implements this interface at the core level.
 * - Thread and other types use this interface to reference graph data.
 * - Provide basic operations and query functions for graphs
 */
export interface Graph {
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
  getNode(nodeId: ID): GraphNode | undefined;
  /** Getting the edge */
  getEdge(edgeId: ID): GraphEdge | undefined;
  /** Get the outgoing neighbors of the node */
  getOutgoingNeighbors(nodeId: ID): Set<ID>;
  /** Get the incoming edge neighbors of the node */
  getIncomingNeighbors(nodeId: ID): Set<ID>;
  /** Get the outgoing edge of the node */
  getOutgoingEdges(nodeId: ID): GraphEdge[];
  /** Get the incoming edge of the node */
  getIncomingEdges(nodeId: ID): GraphEdge[];
  /** Get the edge between two nodes */
  getEdgeBetween(sourceNodeId: ID, targetNodeId: ID): GraphEdge | undefined;
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
  getSourceNodes(): GraphNode[];
  /** Get the node with out degree 0 */
  getSinkNodes(): GraphNode[];
}
