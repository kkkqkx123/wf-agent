/**
 * Workflow Graph Data Class
 * Inherits from WorkflowGraphData, implements the WorkflowGraph interface
 */

import type {
  WorkflowGraph as WorkflowGraphType,
  IdMapping,
  SubgraphRelationship,
  WorkflowGraphAnalysis,
  PreprocessValidationResult,
  SubgraphMergeLog,
  ID,
  Timestamp,
  Version,
  WorkflowTrigger,
  VariableDefinition,
  StaticNodeType,
  EdgeType,
  StaticNode,
} from "@wf-agent/types";
import { WorkflowGraphData } from "./workflow-graph-data.js";

/**
 * Workflow Graph Data Class
 * inherits from WorkflowGraphData, implements the WorkflowGraph interface
 */
export class WorkflowGraph extends WorkflowGraphData implements WorkflowGraphType {
  // ========== ID Mapping Related ----------
  /** ID Mapping Table (Temporary Data during the Construction Phase) */
  public idMapping: IdMapping;

  /** 
   * Preprocessed node configurations (ID references have been updated)
   * Maps node ID to its static node configuration with resolved references
   */
  public nodeConfigs: Map<ID, StaticNode>;

  /** 
   * Processed trigger configurations (ID references have been updated)
   * Maps trigger ID to its workflow trigger configuration
   */
  public triggerConfigs: Map<ID, WorkflowTrigger>;

  /** Sub-workflow relationships */
  public subgraphRelationships: SubgraphRelationship[];

  /** Graph analysis results */
  public graphAnalysis: WorkflowGraphAnalysis;

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
  public variables?: VariableDefinition[];

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
    this.nodeConfigs = new Map<ID, StaticNode>();
    this.triggerConfigs = new Map<ID, WorkflowTrigger>();
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
        byType: new Map<string, number>(),
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

  /**
   * Get node configuration by ID
   * @param nodeId - Node ID
   * @returns Static node configuration or undefined if not found
   */
  public getNodeConfig(nodeId: ID): StaticNode | undefined {
    return this.nodeConfigs.get(nodeId);
  }

  /**
   * Get node configuration by type with type guard
   * @param nodeId - Node ID
   * @param nodeType - Expected static node type
   * @returns Typed static node configuration or undefined if not found or type mismatch
   */
  public getNodeConfigByType<T extends StaticNodeType>(
    nodeId: ID,
    nodeType: T,
  ): Extract<StaticNode, { type: T }> | undefined {
    const config = this.nodeConfigs.get(nodeId);
    if (config && config.type === nodeType) {
      return config as Extract<StaticNode, { type: T }>;
    }
    return undefined;
  }

  /**
   * Check if a node exists and is of specific type
   * @param nodeId - Node ID
   * @param nodeType - Static node type to check
   * @returns True if node exists and matches the type
   */
  public isNodeOfType(nodeId: ID, nodeType: StaticNodeType): boolean {
    const config = this.nodeConfigs.get(nodeId);
    return config !== undefined && config.type === nodeType;
  }

  /**
   * Get all nodes of a specific type
   * @param nodeType - Static node type to filter
   * @returns Array of node IDs matching the type
   */
  public getNodeIdsByType(nodeType: StaticNodeType): ID[] {
    const nodeIds: ID[] = [];
    for (const [nodeId, config] of this.nodeConfigs.entries()) {
      if (config.type === nodeType) {
        nodeIds.push(nodeId);
      }
    }
    return nodeIds;
  }

  /**
   * Get trigger configuration by ID
   * @param triggerId - Trigger ID
   * @returns Workflow trigger configuration or undefined if not found
   */
  public getTriggerConfig(triggerId: ID): WorkflowTrigger | undefined {
    return this.triggerConfigs.get(triggerId);
  }

  /**
   * Add or update node configuration
   * @param nodeId - Node ID
   * @param config - Static node configuration
   */
  public setNodeConfig(nodeId: ID, config: StaticNode): void {
    this.nodeConfigs.set(nodeId, config);
  }

  /**
   * Add or update trigger configuration
   * @param triggerId - Trigger ID
   * @param config - Workflow trigger configuration
   */
  public setTriggerConfig(triggerId: ID, config: WorkflowTrigger): void {
    this.triggerConfigs.set(triggerId, config);
  }
}
