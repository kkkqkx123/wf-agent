/**
 * Preprocessed Graph Data Class
 *  Inherits from GraphData, implements the PreprocessedGraph interface
 */

import type {
  PreprocessedGraph,
  IdMapping,
  SubgraphRelationship,
  GraphAnalysisResult,
  PreprocessValidationResult,
  SubgraphMergeLog,
  ID,
  Timestamp,
  Version,
  WorkflowTrigger,
  WorkflowVariable,
  NodeType,
  EdgeType,
} from "@wf-agent/types";
import { GraphData } from "./graph-data.js";

/**
 * Preprocessed Graph Data Class
 * inherits from GraphData, implements the PreprocessedGraph interface
 */
export class PreprocessedGraphData extends GraphData implements PreprocessedGraph {
  // ========== ID Mapping Related ----------
  /** ID Mapping Table (Temporary Data during the Construction Phase) */
  public idMapping: IdMapping;

  /** Preprocessed node configuration (ID references have been updated) */
  public nodeConfigs: Map<ID, unknown>;

  /** Processed trigger configuration (ID references have been updated). */
  public triggerConfigs: Map<ID, unknown>;

  /** Sub-workflow relationships */
  public subgraphRelationships: SubgraphRelationship[];

  /** Graph analysis results */
  public graphAnalysis: GraphAnalysisResult;

  /** Preprocess the validation results. */
  public validationResult: PreprocessValidationResult;

  /** List of node IDs after topological sorting */
  public topologicalOrder: ID[];

  /** Sub-workflow merge logs */
  public subgraphMergeLogs: SubgraphMergeLog[];

  /** Preprocessing timestamp */
  public processedAt: Timestamp;

  // Workflow Metadata
  /** Workflow ID */
  public workflowId: ID;

  /** Workflow version */
  public workflowVersion: Version;

  /** Trigger (expanded, without references) */
  public triggers?: WorkflowTrigger[];

  /** Workflow variable definitions */
  public variables?: WorkflowVariable[];

  /** Does it contain sub-workflows? */
  public hasSubgraphs: boolean;

  /** Set of sub-workflow IDs */
  public subworkflowIds: Set<ID>;

  constructor() {
    super();

    // Initialize fields related to the ID mapping.
    this.idMapping = {
      nodeIds: new Map(),
      edgeIds: new Map(),
      reverseNodeIds: new Map(),
      reverseEdgeIds: new Map(),
      subgraphNamespaces: new Map(),
    };
    this.nodeConfigs = new Map();
    this.triggerConfigs = new Map();
    this.subgraphRelationships = [];

    // Initialize preprocessing metadata
    this.graphAnalysis = {
      cycleDetection: {
        hasCycle: false,
        cycleNodes: [],
        cycleEdges: [],
      },
      reachability: {
        reachableFromStart: new Set(),
        reachableToEnd: new Set(),
        unreachableNodes: new Set(),
        deadEndNodes: new Set(),
      },
      topologicalSort: {
        success: true,
        sortedNodes: [],
        cycleNodes: [],
      },
      forkJoinValidation: {
        isValid: true,
        unpairedForks: [],
        unpairedJoins: [],
        pairs: new Map(),
      },
      nodeStats: {
        total: 0,
        byType: new Map<NodeType, number>(),
      },
      edgeStats: {
        total: 0,
        byType: new Map<EdgeType, number>(),
      },
    };
    this.validationResult = {
      isValid: true,
      errors: [],
      warnings: [],
      validatedAt: 0,
    };
    this.topologicalOrder = [];
    this.subgraphMergeLogs = [];
    this.processedAt = 0;

    // Initialize workflow metadata
    this.workflowId = "";
    this.workflowVersion = "1.0.0";
    this.hasSubgraphs = false;
    this.subworkflowIds = new Set();
  }
}
