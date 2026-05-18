/**
 * FORK/JOIN Pair Validator
 *
 * Validates FORK and JOIN node pairing and business logic:
 * - Validity of forkPaths configuration for FORK nodes
 * - Validity of forkPathIds and mainPathId configuration for JOIN nodes
 * - Pairing relationship between FORK and JOIN nodes
 * - Global uniqueness of forkPathIds
 * - Reachability from FORK to JOIN
 *
 * Note: Schema validation of node configurations has already been completed in WorkflowValidator.
 * This validator focuses on business logic and pairing relationships.
 */

import type { ID, StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";
import { getReachableNodes } from "../../builder/utils/workflow-traversal.js";

/**
 * Validate FORK/JOIN pairs
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateForkJoinPairs(
  graph: WorkflowGraphData,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];
  
  // Use the first element of forkPathIds array as the pairing identifier
  const forkNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[] }>(); // forkPathIds[0] -> {nodeId, forkPathIds}
  const joinNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[]; mainPathId?: ID }>(); // forkPathIds[0] -> {nodeId, forkPathIds, mainPathId}
  const pairs = new Map<ID, ID>();
  const allForkPathIds = new Set<ID>(); // Used to check global uniqueness of forkPathId

  // Collect all FORK and JOIN nodes
  for (const node of graph.nodes.values()) {
    if (node.type === ("FORK" as StaticNodeType)) {
      const config = node.originalNode?.config as
        | { forkPaths?: Array<{ pathId: ID; childNodeId: ID }> }
        | undefined;
      const forkPaths = config?.forkPaths;

      // Validate Fork node configuration
      if (!forkPaths || !Array.isArray(forkPaths) || forkPaths.length === 0) {
        errors.push(
          new ConfigurationValidationError(`FORK node (${node.id}) forkPaths must be a non-empty array`, {
            configType: "workflow",
            context: {
              code: "INVALID_FORK_PATHS",
              nodeId: node.id,
            },
          }),
        );
        continue;
      }

      // Extract pathId and childNodeId from forkPaths
      const forkPathIds: ID[] = [];
      const childNodeIds: string[] = [];
      for (const forkPath of forkPaths) {
        if (!forkPath.pathId || !forkPath.childNodeId) {
          errors.push(
            new ConfigurationValidationError(
              `Each element in forkPaths of FORK node (${node.id}) must contain pathId and childNodeId`,
              {
                configType: "workflow",
                context: {
                  code: "INVALID_FORK_PATH_ITEM",
                  nodeId: node.id,
                },
              },
            ),
          );
          continue;
        }
        forkPathIds.push(forkPath.pathId);
        childNodeIds.push(forkPath.childNodeId);
      }

      // Check if pathId is unique within the workflow definition
      for (const forkPathId of forkPathIds) {
        if (allForkPathIds.has(forkPathId)) {
          errors.push(
            new ConfigurationValidationError(
              `pathId (${forkPathId}) of FORK node (${node.id}) is not unique within the workflow definition`,
              {
                configType: "workflow",
                context: {
                  code: "DUPLICATE_FORK_PATH_ID",
                  nodeId: node.id,
                  forkPathId,
                },
              },
            ),
          );
        } else {
          allForkPathIds.add(forkPathId);
        }
      }

      // Use the pathId of the first element from forkPaths as the pairing identifier
      if (forkPathIds.length === 0) {
        continue;
      }
      const pairId = forkPathIds[0]!;
      if (forkNodes.has(pairId)) {
        errors.push(
          new ConfigurationValidationError(
            `The pathId (${pairId}) of the first element in forkPaths of FORK node (${node.id}) is already used by another FORK node`,
            {
              configType: "workflow",
              context: {
                code: "DUPLICATE_FORK_PAIR_ID",
                nodeId: node.id,
                pairId,
              },
            },
          ),
        );
      } else {
        forkNodes.set(pairId, { nodeId: node.id, forkPathIds });
      }
    } else if (node.type === ("JOIN" as StaticNodeType)) {
      const config = node.originalNode?.config as
        | { forkPathIds?: ID[]; mainPathId?: ID }
        | undefined;
      const forkPathIds = config?.forkPathIds;
      const mainPathId = config?.mainPathId;

      // Validate Join node configuration
      if (!forkPathIds || !Array.isArray(forkPathIds) || forkPathIds.length === 0) {
        errors.push(
          new ConfigurationValidationError(`forkPathIds of JOIN node (${node.id}) must be a non-empty array`, {
            configType: "workflow",
            context: {
              code: "INVALID_FORK_PATH_IDS",
              nodeId: node.id,
            },
          }),
        );
        continue;
      }

      // Validate mainPathId
      if (mainPathId && !forkPathIds.includes(mainPathId)) {
        errors.push(
          new ConfigurationValidationError(
            `mainPathId (${mainPathId}) of JOIN node (${node.id}) must be in forkPathIds`,
            {
              configType: "workflow",
              context: {
                code: "MAIN_PATH_ID_NOT_FOUND",
                nodeId: node.id,
                mainPathId,
              },
            },
          ),
        );
        continue;
      }

      // Use the first element of forkPathIds as the pairing identifier
      const pairId = forkPathIds[0];
      if (pairId && joinNodes.has(pairId)) {
        errors.push(
          new ConfigurationValidationError(
            `The first element (${pairId}) of forkPathIds of JOIN node (${node.id}) is already used by another JOIN node`,
            {
              configType: "workflow",
              context: {
                code: "DUPLICATE_JOIN_PAIR_ID",
                nodeId: node.id,
                pairId,
              },
            },
          ),
        );
      } else if (pairId) {
        joinNodes.set(pairId, { nodeId: node.id, forkPathIds, mainPathId });
      }
    }
  }

  // Check pairing
  const unpairedForks: ID[] = [];
  const unpairedJoins: ID[] = [];

  for (const [pairId, forkInfo] of forkNodes) {
    if (joinNodes.has(pairId)) {
      const joinInfo = joinNodes.get(pairId)!;
      // Verify that forkPathIds arrays of Fork and Join are identical (including order)
      if (JSON.stringify(forkInfo.forkPathIds) !== JSON.stringify(joinInfo.forkPathIds)) {
        errors.push(
          new ConfigurationValidationError(
            `forkPathIds of FORK node (${forkInfo.nodeId}) and JOIN node (${joinInfo.nodeId}) do not match`,
            {
              configType: "workflow",
              context: {
                code: "FORK_JOIN_MISMATCH",
                forkNodeId: forkInfo.nodeId,
                joinNodeId: joinInfo.nodeId,
              },
            },
          ),
        );
      } else {
        pairs.set(forkInfo.nodeId, joinInfo.nodeId);
      }
    } else {
      unpairedForks.push(forkInfo.nodeId);
    }
  }

  for (const [pairId, joinInfo] of joinNodes) {
    if (!forkNodes.has(pairId)) {
      unpairedJoins.push(joinInfo.nodeId);
    }
  }

  // Report unmatched FORK nodes
  for (const forkNodeId of unpairedForks) {
    errors.push(
      new ConfigurationValidationError(`FORK node (${forkNodeId}) has no matching JOIN node`, {
        configType: "workflow",
        context: {
          code: "UNPAIRED_FORK",
          nodeId: forkNodeId,
        },
      }),
    );
  }

  // Report unmatched JOIN nodes
  for (const joinNodeId of unpairedJoins) {
    errors.push(
      new ConfigurationValidationError(`JOIN node (${joinNodeId}) has no matching FORK node`, {
        configType: "workflow",
        context: {
          code: "UNPAIRED_JOIN",
          nodeId: joinNodeId,
        },
      }),
    );
  }

  // Check reachability from FORK to JOIN
  for (const [forkNodeId, joinNodeId] of pairs) {
    const reachableNodes = getReachableNodes(graph, forkNodeId);
    if (!reachableNodes.has(joinNodeId)) {
      errors.push(
        new ConfigurationValidationError(
          `FORK node (${forkNodeId}) cannot reach the paired JOIN node (${joinNodeId})`,
          {
            configType: "workflow",
            context: {
              code: "FORK_JOIN_NOT_REACHABLE",
              nodeId: forkNodeId,
              relatedNodeId: joinNodeId,
            },
          },
        ),
      );
    }
  }

  return errors;
}
