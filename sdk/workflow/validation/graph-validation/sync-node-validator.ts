/**
 * SYNC Node Validator
 *
 * Validates SYNC nodes configuration and pairing:
 * - SYNC nodes must be within a FORK-JOIN branch structure
 * - sourcePathId and targetPathId must exist in parent FORK node's forkPaths
 * - variableMappings format is valid
 * - SYNC nodes are not isolated (have incoming/outgoing edges)
 */

import type { ID, StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";

/**
 * Validate SYNC nodes configuration and pairing
 * @param graph Graph data containing SYNC nodes
 * @returns List of validation errors
 */
export function validateSyncNodes(
  graph: WorkflowGraphData,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Collect all SYNC nodes
  const syncNodes: Array<{
    nodeId: ID;
    config: {
      sourcePathId?: ID;
      targetPathId?: ID;
      variableMappings?: Array<{ externalName: string; internalName: string }>;
    };
  }> = [];

  for (const node of graph.nodes.values()) {
    if (node.type === ("SYNC" as StaticNodeType)) {
      const config = node.originalNode?.config as {
        sourcePathId?: ID;
        targetPathId?: ID;
        variableMappings?: Array<{ externalName: string; internalName: string }>;
      } | undefined;

      if (config) {
        syncNodes.push({
          nodeId: node.id,
          config: {
            sourcePathId: config.sourcePathId,
            targetPathId: config.targetPathId,
            variableMappings: config.variableMappings,
          },
        });
      }
    }
  }

  // If no SYNC nodes, skip validation
  if (syncNodes.length === 0) {
    return errors;
  }

  // Build FORK node path mapping (forkPathIds -> forkNodeId)
  const forkPathMapping = new Map<ID, ID>();
  for (const node of graph.nodes.values()) {
    if (node.type === ("FORK" as StaticNodeType)) {
      const config = node.originalNode?.config as {
        forkPaths?: Array<{ pathId: ID; childNodeId: ID }>;
      } | undefined;

      if (config?.forkPaths) {
        for (const forkPath of config.forkPaths) {
          forkPathMapping.set(forkPath.pathId, node.id);
        }
      }
    }
  }

  // Validate each SYNC node
  for (const syncNode of syncNodes) {
    const { nodeId, config } = syncNode;

    // Check if sourcePathId is present
    if (!config.sourcePathId || !config.sourcePathId.trim()) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' is missing required sourcePathId`,
          {
            configType: "workflow",
            context: {
              code: "MISSING_SYNC_SOURCE_PATH_ID",
              nodeId,
            },
          }
        )
      );
      continue;
    }

    // Verify sourcePathId exists in a FORK node's forkPaths
    if (!forkPathMapping.has(config.sourcePathId)) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' has sourcePathId '${config.sourcePathId}' that does not exist in any FORK node's forkPaths`,
          {
            configType: "workflow",
            context: {
              code: "INVALID_SYNC_SOURCE_PATH_ID",
              nodeId,
              sourcePathId: config.sourcePathId,
            },
          }
        )
      );
    }

    // Verify targetPathId if provided
    if (config.targetPathId && config.targetPathId.trim()) {
      if (!forkPathMapping.has(config.targetPathId)) {
        errors.push(
          new ConfigurationValidationError(
            `SYNC node '${nodeId}' has targetPathId '${config.targetPathId}' that does not exist in any FORK node's forkPaths`,
            {
              configType: "workflow",
              context: {
                code: "INVALID_SYNC_TARGET_PATH_ID",
                nodeId,
                targetPathId: config.targetPathId,
              },
            }
          )
        );
      }
    }

    // Validate variableMappings format
    if (config.variableMappings && config.variableMappings.length > 0) {
      for (const mapping of config.variableMappings) {
        if (!mapping.externalName || !mapping.externalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SYNC node '${nodeId}' has variableMapping with missing externalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_SYNC_MAPPING_EXTERNAL_NAME",
                  nodeId,
                  internalName: mapping.internalName,
                },
              }
            )
          );
        }

        if (!mapping.internalName || !mapping.internalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SYNC node '${nodeId}' has variableMapping with missing internalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_SYNC_MAPPING_INTERNAL_NAME",
                  nodeId,
                  externalName: mapping.externalName,
                },
              }
            )
          );
        }
      }
    }

    // Check if SYNC node is isolated (no incoming or outgoing edges)
    const incomingEdges = graph.getIncomingEdges(nodeId);
    const outgoingEdges = graph.getOutgoingEdges(nodeId);
    if (incomingEdges.length === 0 && outgoingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' is isolated, has no incoming or outgoing edges`,
          {
            configType: "workflow",
            context: {
              code: "ISOLATED_SYNC_NODE",
              nodeId,
            },
          }
        )
      );
    }
  }

  return errors;
}
