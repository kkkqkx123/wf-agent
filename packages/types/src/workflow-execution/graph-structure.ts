/**
 * Graph Structure Type Definition
 *
 * NOTE: This module is placed under the workflow-execution directory because
 * WorkflowGraphStructure is tightly coupled with WorkflowExecution.
 * It defines the runtime graph structure used during workflow execution.
 */

import type { ID } from "../common.js";
import type { StaticNode, RuntimeNode } from "../node/index.js";
import type { Edge, EdgeType } from "../workflow/edge.js";
import type { Condition } from "../condition.js";

/**
 * Workflow Node Type
 *
 * Represents nodes in the workflow execution graph AFTER preprocessing.
 * This is a RuntimeNode with additional graph-specific properties.
 *
 * IMPORTANT: Display metadata (name/description) are NOT part of RuntimeNode.
 * They are optionally copied here from StaticNode for logging/debugging convenience.
 * For CRUD/UI operations, always use the originalNode reference.
 *
 * This is used during:
 * - Graph building and preprocessing
 * - Graph validation and analysis
 * - Workflow execution (nodes retrieved from the graph)
 */
export type WorkflowNode = RuntimeNode & {
  /** Reference to the original static node definition (for accessing display props if needed) */
  originalNode?: StaticNode;

  /**
   * Optional node name copied from static node for logging/debugging convenience.
   * NOTE: Prefer accessing via originalNode?.name for better separation of concerns.
   * This field may be removed in future versions.
   */
  name?: string;
};

/**
 * Workflow Edge Types
 * Edge representations for graph validation and analysis
 */
export interface WorkflowEdge {
  id: ID;
  sourceNodeId: ID;
  targetNodeId: ID;
  type: EdgeType;
  label?: string;
  description?: string;
  weight?: number;
  condition?: Condition;
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
