/**
 * Node ConfigurationUpdater utility functions
 * Provide specialized ID mapping utility functions for each type of node.
 */

import type { NodeConfigUpdater, IdMapping, Node } from "@wf-agent/types";
import type { ID } from "@wf-agent/types";
import { NodeType } from "@wf-agent/types";

/**
 * Map node IDs
 */
function mapNodeId(originalId: ID, idMapping: IdMapping): ID {
  const index = idMapping.nodeIds.get(originalId);
  if (index === undefined) {
    return originalId;
  }
  return index.toString();
}

/**
 * Mapping path ID
 */
function mapPathId(originalId: ID, idMapping: IdMapping): ID {
  const index = idMapping.edgeIds.get(originalId);
  if (index === undefined) {
    return originalId;
  }
  return index.toString();
}

/**
 * ROUTE Node ConfigurationUpdater
 * Handles the targetNodeId and defaultTargetNodeId within the ROUTE node configuration.
 */
const routeNodeConfigUpdater: NodeConfigUpdater = {
  nodeType: "ROUTE",

  containsIdReferences(config: unknown): boolean {
    if (!config || typeof config !== "object") {
      return false;
    }
    const cfg = config as { routes?: Array<{ targetNodeId?: ID }>; defaultTargetNodeId?: ID };
    if (!cfg.routes) {
      return false;
    }

    // Check the `targetNodeId` in the `routes`.
    for (const route of cfg.routes) {
      if (route.targetNodeId) {
        return true;
      }
    }

    // Check defaultTargetNodeId
    if (cfg.defaultTargetNodeId) {
      return true;
    }

    return false;
  },

  updateIdReferences(config: unknown, idMapping: IdMapping): unknown {
    if (!config || typeof config !== "object") {
      return config;
    }
    const cfg = config as { routes?: Array<{ targetNodeId?: ID }>; defaultTargetNodeId?: ID };

    const updatedRoutes =
      cfg.routes?.map((route: { targetNodeId?: ID }) => ({
        ...route,
        targetNodeId: route.targetNodeId ? mapNodeId(route.targetNodeId, idMapping) : undefined,
      })) || [];

    const updatedDefaultTargetNodeId = cfg.defaultTargetNodeId
      ? mapNodeId(cfg.defaultTargetNodeId, idMapping)
      : undefined;

    return {
      ...cfg,
      routes: updatedRoutes,
      defaultTargetNodeId: updatedDefaultTargetNodeId,
    };
  },
};

/**
 * FORK node configuration updater
 * Processes forkPaths.pathId in FORK node configurations
 */
const forkNodeConfigUpdater: NodeConfigUpdater = {
  nodeType: "FORK",

  containsIdReferences(config: unknown): boolean {
    if (!config || typeof config !== "object") {
      return false;
    }
    const cfg = config as { forkPaths?: Array<{ pathId?: ID }> };
    if (!cfg.forkPaths) {
      return false;
    }

    for (const forkPath of cfg.forkPaths) {
      if (forkPath.pathId) {
        return true;
      }
    }

    return false;
  },

  updateIdReferences(config: unknown, idMapping: IdMapping): unknown {
    if (!config || typeof config !== "object") {
      return config;
    }
    const cfg = config as { forkPaths?: Array<{ pathId?: ID }> };

    const updatedForkPaths =
      cfg.forkPaths?.map((forkPath: { pathId?: ID }) => ({
        ...forkPath,
        pathId: forkPath.pathId ? mapPathId(forkPath.pathId, idMapping) : undefined,
      })) || [];

    return {
      ...cfg,
      forkPaths: updatedForkPaths,
    };
  },
};

/**
 * JOIN Node ConfigurationUpdater
 * Handles the forkPathIds and mainPathId parameters in JOIN node configurations
 */
const joinNodeConfigUpdater: NodeConfigUpdater = {
  nodeType: "JOIN",

  containsIdReferences(config: unknown): boolean {
    if (!config || typeof config !== "object") {
      return false;
    }
    const cfg = config as { forkPathIds?: ID[]; mainPathId?: ID };

    if (cfg.forkPathIds && cfg.forkPathIds.length > 0) {
      return true;
    }

    if (cfg.mainPathId) {
      return true;
    }

    return false;
  },

  updateIdReferences(config: unknown, idMapping: IdMapping): unknown {
    if (!config || typeof config !== "object") {
      return config;
    }
    const cfg = config as { forkPathIds?: ID[]; mainPathId?: ID };

    const updatedForkPathIds = cfg.forkPathIds?.map((id: ID) => mapPathId(id, idMapping)) || [];

    const updatedMainPathId = cfg.mainPathId ? mapPathId(cfg.mainPathId, idMapping) : undefined;

    return {
      ...cfg,
      forkPathIds: updatedForkPathIds,
      mainPathId: updatedMainPathId,
    };
  },
};

/**
 * Subgraph Node ConfigurationUpdater
 * Handles the subgraphId in the SUBGRAPH node configuration
 * Note: subgraphId refers to the workflow ID, not the node ID, so no mapping is required.
 */
const subgraphNodeConfigUpdater: NodeConfigUpdater = {
  nodeType: "SUBGRAPH",

  containsIdReferences(config: unknown): boolean {
    if (!config || typeof config !== "object") {
      return false;
    }
    const cfg = config as { subgraphId?: string };
    if (!cfg.subgraphId) {
      return false;
    }
    return true;
  },

  updateIdReferences(config: unknown): unknown {
    if (!config || typeof config !== "object") {
      return config;
    }

    // The `subgraphId` of the SUBGRAPH node does not need to be mapped, as it refers to the workflow ID, not the node ID.
    return config;
  },
};

/**
 * Node ConfigurationUpdater Mapping Table
 */
const nodeConfigUpdaters: Partial<Record<NodeType, NodeConfigUpdater>> = {
  ["ROUTE"]: routeNodeConfigUpdater,
  ["FORK"]: forkNodeConfigUpdater,
  ["JOIN"]: joinNodeConfigUpdater,
  ["SUBGRAPH"]: subgraphNodeConfigUpdater,
};

/**
 * Get the configuration updater for the specified node type
 * @param nodeType: The node type
 * @returns: The node configuration updater; returns undefined if it does not exist
 */
export function getNodeConfigUpdater(nodeType: NodeType): NodeConfigUpdater | undefined {
  return nodeConfigUpdaters[nodeType];
}

/**
 * Check if the node configuration contains ID references.
 * @param node: The node
 * @returns: Whether ID references are present
 */
export function containsIdReferences(node: Node): boolean {
  const updater = getNodeConfigUpdater(node.type);
  if (!updater) {
    return false;
  }
  return updater.containsIdReferences(node.config);
}

/**
 * Update the ID references in the node configuration
 * @param node The node
 * @param idMapping The ID mapping table
 * @returns The updated node
 */
export function updateIdReferences(node: Node, idMapping: IdMapping): Node {
  const updater = getNodeConfigUpdater(node.type);
  if (!updater) {
    return node;
  }

  const updatedConfig = updater.updateIdReferences(node.config, idMapping);

  return {
    ...node,
    config: updatedConfig as Node["config"],
  } as Node;
}

/**
 * Get all supported node types
 * @returns Array of node types
 */
export function getSupportedNodeTypes(): NodeType[] {
  return Object.keys(nodeConfigUpdaters) as NodeType[];
}

/**
 * Check if an updater for the specified node type is supported.
 * @param nodeType: The node type
 * @returns: Whether support is available
 */
export function isNodeTypeSupported(nodeType: NodeType): boolean {
  return nodeType in nodeConfigUpdaters;
}
