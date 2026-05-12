/**
 * Subgraph boundary tagging constants
 * 
 * @deprecated These metadata keys were used in the old architecture where START/END 
 * nodes were marked via internalMetadata. Now we use proper type conversion:
 * - START → SUBGRAPH_START
 * - END → SUBGRAPH_END
 * 
 * Use isSubgraphStartNode() and isSubgraphEndNode() type guards instead.
 */

import type { ID } from "../common.js";

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
