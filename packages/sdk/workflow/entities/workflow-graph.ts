/**
 * Workflow Graph
 *
 * Design Notes:
 * - Uses composition to combine graph structure with metadata
 * - Clean separation between immutable structure and mutable metadata
 * - Provides unified interface for workflow graph operations
 *
 * Core Responsibilities:
 * - Combines WorkflowGraphStructure (immutable) with WorkflowGraphMetadata (mutable)
 * - Provides convenient access to both structural and metadata information
 * - Maintains clear lifecycle: construction -> preprocessing -> runtime
 *
 * Usage:
 * - Main interface for workflow graph operations
 * - Used by all workflow components (builders, validators, executors)
 * - Provides both graph traversal and metadata access
 */

import type {
  ID,
  Timestamp,
  Version,
  WorkflowNode,
  WorkflowEdge,
  WorkflowTrigger,
  VariableDefinition,
  StaticNode,
} from "@wf-agent/types";
import type { WorkflowGraphStructure as WorkflowGraphStructureType } from "@wf-agent/types";
import type { WorkflowGraphAnalysis } from "../types/graph/analysis.js";
import type {
  IdMapping,
  SubgraphRelationship,
  SubgraphMergeLog,
  PreprocessValidationResult,
} from "../types/preprocess.js";
import { WorkflowGraphStructureImpl } from "./workflow-graph-structure.js";
import { WorkflowGraphMetadata } from "./workflow-graph-metadata.js";

/**
 * Workflow Graph
 * Uses composition to combine structure with metadata
 */
export class WorkflowGraph implements WorkflowGraphStructureType {
  /** Immutable graph structure */
  public readonly structure: WorkflowGraphStructureImpl;

  /** Mutable metadata for preprocessing and analysis */
  public readonly metadata: WorkflowGraphMetadata;

  constructor(structure?: WorkflowGraphStructureImpl, metadata?: WorkflowGraphMetadata) {
    this.structure = structure ?? new WorkflowGraphStructureImpl();
    this.metadata = metadata ?? new WorkflowGraphMetadata();
  }

  // ========== Delegated Structure Methods ==========

  /** Node set */
  get nodes(): Map<ID, WorkflowNode> {
    return this.structure.nodes;
  }

  /** Edge Set */
  get edges(): Map<ID, WorkflowEdge> {
    return this.structure.edges;
  }

  /** Forward Adjacency List */
  get adjacencyList(): Map<ID, Set<ID>> {
    return this.structure.adjacencyList;
  }

  /** Reverse Adjacency List */
  get reverseAdjacencyList(): Map<ID, Set<ID>> {
    return this.structure.reverseAdjacencyList;
  }

  /** Starting node ID */
  get startNodeId(): ID | undefined {
    return this.structure.startNodeId;
  }

  /** End node ID set */
  get endNodeIds(): Set<ID> {
    return this.structure.endNodeIds;
  }

  /**
   * Add a node (delegated to structure)
   */
  addNode(node: WorkflowNode): void {
    this.structure.addNode(node);
  }

  /**
   * Add edges (delegated to structure)
   */
  addEdge(edge: WorkflowEdge): void {
    this.structure.addEdge(edge);
  }

  /**
   * Get the node (delegated to structure)
   */
  getNode(nodeId: ID): WorkflowNode | undefined {
    return this.structure.getNode(nodeId);
  }

  /**
   * Get the edges (delegated to structure)
   */
  getEdge(edgeId: ID): WorkflowEdge | undefined {
    return this.structure.getEdge(edgeId);
  }

  /**
   * Get the out-degree neighbors of a node (delegated to structure)
   */
  getOutgoingNeighbors(nodeId: ID): Set<ID> {
    return this.structure.getOutgoingNeighbors(nodeId);
  }

  /**
   * Get the in-degree neighbors of a node (delegated to structure)
   */
  getIncomingNeighbors(nodeId: ID): Set<ID> {
    return this.structure.getIncomingNeighbors(nodeId);
  }

  /**
   * Get the outgoing edges of a node (delegated to structure)
   */
  getOutgoingEdges(nodeId: ID): WorkflowEdge[] {
    return this.structure.getOutgoingEdges(nodeId);
  }

  /**
   * Get the incoming edges of a node (delegated to structure)
   */
  getIncomingEdges(nodeId: ID): WorkflowEdge[] {
    return this.structure.getIncomingEdges(nodeId);
  }

  /**
   * Get the edge between two nodes (delegated to structure)
   */
  getEdgeBetween(sourceNodeId: ID, targetNodeId: ID): WorkflowEdge | undefined {
    return this.structure.getEdgeBetween(sourceNodeId, targetNodeId);
  }

  /**
   * Check if the node exists (delegated to structure)
   */
  hasNode(nodeId: ID): boolean {
    return this.structure.hasNode(nodeId);
  }

  /**
   * Check if the edge exists (delegated to structure)
   */
  hasEdge(edgeId: ID): boolean {
    return this.structure.hasEdge(edgeId);
  }

  /**
   * Check if there is an edge between two nodes (delegated to structure)
   */
  hasEdgeBetween(sourceNodeId: ID, targetNodeId: ID): boolean {
    return this.structure.hasEdgeBetween(sourceNodeId, targetNodeId);
  }

  /**
   * Get all node IDs (delegated to structure)
   */
  getAllNodeIds(): ID[] {
    return this.structure.getAllNodeIds();
  }

  /**
   * Get all edge IDs (delegated to structure)
   */
  getAllEdgeIds(): ID[] {
    return this.structure.getAllEdgeIds();
  }

  /**
   * Get the number of nodes (delegated to structure)
   */
  getNodeCount(): number {
    return this.structure.getNodeCount();
  }

  /**
   * Get the number of edges (delegated to structure)
   */
  getEdgeCount(): number {
    return this.structure.getEdgeCount();
  }

  /**
   * Get source nodes (nodes with in-degree 0) (delegated to structure)
   */
  getSourceNodes(): WorkflowNode[] {
    return this.structure.getSourceNodes();
  }

  /**
   * Get sink nodes (nodes with out-degree 0) (delegated to structure)
   */
  getSinkNodes(): WorkflowNode[] {
    return this.structure.getSinkNodes();
  }

  // ========== Metadata Access Methods ==========

  /** Workflow ID (from metadata) */
  get workflowId(): ID {
    return this.metadata.workflowId;
  }

  /** Workflow version (from metadata) */
  get workflowVersion(): Version {
    return this.metadata.workflowVersion;
  }

  /** ID mapping (from metadata) */
  get idMapping(): IdMapping {
    return this.metadata.idMapping;
  }

  /** Node configurations (from metadata) */
  get nodeConfigs(): Map<ID, StaticNode> {
    return this.metadata.nodeConfigs;
  }

  /** Trigger configurations (from metadata) */
  get triggerConfigs(): Map<ID, WorkflowTrigger> {
    return this.metadata.triggerConfigs;
  }

  /** Graph analysis (from metadata) */
  get graphAnalysis(): WorkflowGraphAnalysis {
    return this.metadata.graphAnalysis;
  }

  /** Validation result (from metadata) */
  get validationResult(): PreprocessValidationResult {
    return this.metadata.validationResult;
  }

  /** Topological order (from metadata) */
  get topologicalOrder(): ID[] {
    return this.metadata.topologicalOrder;
  }

  /** Subgraph relationships (from metadata) */
  get subgraphRelationships(): SubgraphRelationship[] {
    return this.metadata.subgraphRelationships;
  }

  /** Subgraph merge logs (from metadata) */
  get subgraphMergeLogs(): SubgraphMergeLog[] {
    return this.metadata.subgraphMergeLogs;
  }

  /** Processed timestamp (from metadata) */
  get processedAt(): Timestamp {
    return this.metadata.processedAt;
  }

  /** Triggers (from metadata) */
  get triggers(): WorkflowTrigger[] | undefined {
    return this.metadata.triggers;
  }

  /** Variables (from metadata) */
  get variables(): VariableDefinition[] | undefined {
    return this.metadata.variables;
  }

  /** Has subgraphs flag (from metadata) */
  get hasSubgraphs(): boolean {
    return this.metadata.hasSubgraphs;
  }

  /** Subworkflow IDs (from metadata) */
  get subworkflowIds(): Set<ID> {
    return this.metadata.subworkflowIds;
  }

  /**
   * Get node configuration by ID (from metadata)
   */
  getNodeConfig(nodeId: ID) {
    return this.metadata.getNodeConfig(nodeId);
  }

  /**
   * Get node configuration by type with type guard (from metadata)
   */
  getNodeConfigByType<T extends import("@wf-agent/types").StaticNodeType>(
    nodeId: ID,
    nodeType: T,
  ): Extract<import("@wf-agent/types").StaticNode, { type: T }> | undefined {
    return this.metadata.getNodeConfigByType(nodeId, nodeType);
  }

  /**
   * Check if a node exists and is of specific type (from metadata)
   */
  isNodeOfType(nodeId: ID, nodeType: import("@wf-agent/types").StaticNodeType): boolean {
    return this.metadata.isNodeOfType(nodeId, nodeType);
  }

  /**
   * Get all nodes of a specific type (from metadata)
   */
  getNodeIdsByType(nodeType: import("@wf-agent/types").StaticNodeType): ID[] {
    return this.metadata.getNodeIdsByType(nodeType);
  }

  /**
   * Get trigger configuration by ID (from metadata)
   */
  getTriggerConfig(triggerId: ID) {
    return this.metadata.getTriggerConfig(triggerId);
  }

  /**
   * Add or update node configuration (in metadata)
   */
  setNodeConfig(nodeId: ID, config: import("@wf-agent/types").StaticNode): void {
    this.metadata.setNodeConfig(nodeId, config);
  }

  /**
   * Add or update trigger configuration (in metadata)
   */
  setTriggerConfig(triggerId: ID, config: import("@wf-agent/types").WorkflowTrigger): void {
    this.metadata.setTriggerConfig(triggerId, config);
  }

  /**
   * Check if preprocessing is complete
   */
  isPreprocessed(): boolean {
    return this.metadata.isPreprocessed();
  }

  /**
   * Create a copy of the graph with new structure (for transformations)
   */
  withStructure(newStructure: WorkflowGraphStructureImpl): WorkflowGraph {
    return new WorkflowGraph(newStructure, this.metadata);
  }

  /**
   * Create a copy of the graph with new metadata (for transformations)
   */
  withMetadata(newMetadata: WorkflowGraphMetadata): WorkflowGraph {
    return new WorkflowGraph(this.structure, newMetadata);
  }
}