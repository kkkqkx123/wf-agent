/**
 * Subgraph boundary tagging constants
 * Define keys used in metadata to avoid naming conflicts
 */

import type { ID } from "../common.js";

/**
 * Subgraph boundary marker metadata key name constant
 */
export const SUBGRAPH_METADATA_KEYS = {
  /** Boundary types: 'entry' | 'exit' | 'internal' */
  BOUNDARY_TYPE: "subgraphBoundaryType",
  /** Corresponding original SUBGRAPH node IDs */
  ORIGINAL_NODE_ID: "originalSubgraphNodeId",
  /** subgraph namespace (computing) */
  NAMESPACE: "subgraphNamespace",
  /** subgraph depth */
  DEPTH: "subgraphDepth",
} as const;

/**
 * Subgraph boundary types
 */
export type SubgraphBoundaryType = "entry" | "exit" | "internal";

/**
 * Subgraph Boundary Metadata Interface
 */
export interface SubgraphBoundaryMetadata {
  /** Boundary type */
  boundaryType: SubgraphBoundaryType;
  /** Corresponding original SUBGRAPH node IDs */
  originalSubgraphNodeId: ID;
  /** subgraph namespace (computing) */
  namespace: string;
  /** subgraph depth */
  depth: number;
}
