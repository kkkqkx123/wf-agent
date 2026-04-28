/**
 * ID mapping related type definitions
 * ID mapping and configuration updates for preprocessing stages
 */

import type { ID } from "../common.js";
import type { NodeType } from "../node/base.js";

/**
 * ID Mapping Table
 * Records the mapping relationship from the original ID to the indexed IDs
 */
export interface IdMapping {
  /** Node ID mapping: Raw ID -> Index ID */
  nodeIds: Map<ID, number>;

  /** Edge ID mapping: raw ID -> indexed IDs */
  edgeIds: Map<ID, number>;

  /** Reverse mapping: Index ID -> Original ID */
  reverseNodeIds: Map<number, ID>;
  reverseEdgeIds: Map<number, ID>;

  /** Subgraph Namespace Mapping */
  subgraphNamespaces: Map<ID, string>;
}

/**
 * Node Configuration Updater Interface
 * Defines how to update ID references in node configurations
 */
export interface NodeConfigUpdater {
  /** Node type */
  nodeType: NodeType;

  /**
   * Check if the configuration contains an ID reference
   * @param config node configuration
   * @returns Whether the configuration contains an ID reference
   */
  containsIdReferences(config: unknown): boolean;

  /**
   * Update ID references in the configuration
   * @param config Node configuration
   * @param idMapping ID mapping table
   * @returns Updated configuration
   */
  updateIdReferences(config: unknown, idMapping: IdMapping): unknown;
}

/**
 * Subgraph Relationship Types
 */
export interface SubgraphRelationship {
  /** Parent workflow ID */
  parentWorkflowId: ID;
  /** SUBGRAPH node ID */
  subgraphNodeId: ID;
  /** Subworkflow ID */
  childWorkflowId: ID;
  /** subgraph namespace (computing) */
  namespace: string;
}
