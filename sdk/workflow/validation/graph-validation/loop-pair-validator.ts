/**
 * LOOP_START/LOOP_END Pair Validator
 *
 * Validates LOOP_START and LOOP_END node pairing and topology:
 * - LOOP_START nodes must have a matching LOOP_END node (same loopId)
 * - Each loopId must be uniquely paired (no duplicate LOOP_START or LOOP_END)
 * - LOOP_END's loopStartNodeId must reference an existing LOOP_START node
 * - LOOP_START's loopId and LOOP_END's loopId must match
 *
 * Note: Schema validation of node configurations has already been completed in WorkflowValidator.
 * This validator focuses on graph-level pairing and cross-node references.
 */

import type { ID, StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";

/**
 * Validate LOOP_START/LOOP_END pairs
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateLoopPairs(graph: WorkflowGraphData): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Collect LOOP_START nodes: nodeId -> { loopId }
  const loopStarts = new Map<ID, { loopId: ID }>(); // keyed by node ID
  // Collect LOOP_END nodes: nodeId -> { loopId, loopStartNodeId? }
  const loopEnds = new Map<ID, { loopId: ID; loopStartNodeId?: ID }>(); // keyed by node ID

  // Collect all LOOP_START and LOOP_END nodes
  for (const node of graph.nodes.values()) {
    if (node.type === ("LOOP_START" as StaticNodeType)) {
      const loopId = (node.originalNode?.config as { loopId?: ID } | undefined)?.loopId;
      if (!loopId) {
        errors.push(
          new ConfigurationValidationError(
            `LOOP_START node (${node.id}) must have a non-empty loopId in its config`,
            {
              configType: "workflow",
              context: {
                code: "LOOP_START_MISSING_LOOP_ID",
                nodeId: node.id,
              },
            },
          ),
        );
        continue; // Skip pairing for this node
      }
      loopStarts.set(node.id, { loopId });
    } else if (node.type === ("LOOP_END" as StaticNodeType)) {
      const config = node.originalNode?.config as { loopId?: ID; loopStartNodeId?: ID } | undefined;
      const loopId = config?.loopId;
      const loopStartNodeId = config?.loopStartNodeId;

      if (!loopId) {
        errors.push(
          new ConfigurationValidationError(
            `LOOP_END node (${node.id}) must have a non-empty loopId in its config`,
            {
              configType: "workflow",
              context: {
                code: "LOOP_END_MISSING_LOOP_ID",
                nodeId: node.id,
              },
            },
          ),
        );
        // Still collect with placeholder for cross-reference checks
        loopEnds.set(node.id, { loopId: "", loopStartNodeId });
      } else {
        loopEnds.set(node.id, { loopId, loopStartNodeId });
      }
    }
  }

  // --- Build pairing maps by loopId ---

  // Track loopIds seen on LOOP_START nodes (loopId -> [nodeId])
  const startLoopIds = new Map<ID, ID[]>(); // loopId -> LOOP_START node IDs
  // Track loopIds seen on LOOP_END nodes (loopId -> [nodeId])
  const endLoopIds = new Map<ID, ID[]>(); // loopId -> LOOP_END node IDs

  for (const [nodeId, info] of loopStarts) {
    const ids = startLoopIds.get(info.loopId) ?? [];
    ids.push(nodeId);
    startLoopIds.set(info.loopId, ids);
  }

  for (const [nodeId, info] of loopEnds) {
    if (!info.loopId) continue; // Already reported missing loopId
    const ids = endLoopIds.get(info.loopId) ?? [];
    ids.push(nodeId);
    endLoopIds.set(info.loopId, ids);
  }

  // --- Detect duplicate LOOP_START with the same loopId ---
  for (const [loopId, nodeIds] of startLoopIds) {
    if (nodeIds.length > 1) {
      errors.push(
        new ConfigurationValidationError(
          `Multiple LOOP_START nodes share the same loopId (${loopId}): [${nodeIds.join(", ")}]. Each loop must have a unique loopId.`,
          {
            configType: "workflow",
            context: {
              code: "DUPLICATE_LOOP_START_LOOP_ID",
              loopId,
              nodeIds,
            },
          },
        ),
      );
    }
  }

  // --- Detect duplicate LOOP_END with the same loopId ---
  for (const [loopId, nodeIds] of endLoopIds) {
    if (nodeIds.length > 1) {
      errors.push(
        new ConfigurationValidationError(
          `Multiple LOOP_END nodes share the same loopId (${loopId}): [${nodeIds.join(", ")}]. Each loop must have a unique loop pairing.`,
          {
            configType: "workflow",
            context: {
              code: "DUPLICATE_LOOP_END_LOOP_ID",
              loopId,
              nodeIds,
            },
          },
        ),
      );
    }
  }

  // --- Check pairing: each LOOP_START's loopId must have a matching LOOP_END ---
  const pairedLoopIds = new Set<ID>();

  for (const [loopId, startNodeIds] of startLoopIds) {
    // If we already reported duplicate, skip further checks for this loopId
    if (startNodeIds.length > 1) continue;

    const startNodeId = startNodeIds[0]!;
    const endNodeIds = endLoopIds.get(loopId) ?? [];

    if (endNodeIds.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_START node (${startNodeId}) with loopId (${loopId}) has no matching LOOP_END node`,
          {
            configType: "workflow",
            context: {
              code: "UNPAIRED_LOOP_START",
              nodeId: startNodeId,
              loopId,
            },
          },
        ),
      );
    } else if (endNodeIds.length > 1) {
      // Duplicate already reported; no need to repeat here
    } else {
      pairedLoopIds.add(loopId);
    }
  }

  // --- Check pairing: each LOOP_END's loopId must have a matching LOOP_START ---
  for (const [loopId, endNodeIds] of endLoopIds) {
    if (endNodeIds.length > 1) continue;
    const endNodeId = endNodeIds[0]!;
    const startNodeIds = startLoopIds.get(loopId) ?? [];

    if (startNodeIds.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_END node (${endNodeId}) with loopId (${loopId}) has no matching LOOP_START node`,
          {
            configType: "workflow",
            context: {
              code: "UNPAIRED_LOOP_END",
              nodeId: endNodeId,
              loopId,
            },
          },
        ),
      );
    }
  }

  // --- Validate LOOP_END's loopStartNodeId references ---
  const allLoopStartNodeIds = new Set(loopStarts.keys());

  for (const [endNodeId, info] of loopEnds) {
    if (!info.loopStartNodeId) continue; // Optional field

    if (!allLoopStartNodeIds.has(info.loopStartNodeId)) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_END node (${endNodeId}) references non-existent LOOP_START node (${info.loopStartNodeId}) via loopStartNodeId`,
          {
            configType: "workflow",
            context: {
              code: "INVALID_LOOP_START_NODE_REFERENCE",
              nodeId: endNodeId,
              loopStartNodeId: info.loopStartNodeId,
            },
          },
        ),
      );
    } else {
      // If loopStartNodeId points to a valid LOOP_START, check that their loopIds match
      const referencedLoopStart = loopStarts.get(info.loopStartNodeId);
      if (referencedLoopStart && info.loopId && referencedLoopStart.loopId !== info.loopId) {
        errors.push(
          new ConfigurationValidationError(
            `LOOP_END node (${endNodeId}) loopId (${info.loopId}) does not match the loopId (${referencedLoopStart.loopId}) of the referenced LOOP_START node (${info.loopStartNodeId})`,
            {
              configType: "workflow",
              context: {
                code: "LOOP_ID_MISMATCH",
                nodeId: endNodeId,
                loopId: info.loopId,
                loopStartNodeId: info.loopStartNodeId,
                referencedLoopId: referencedLoopStart.loopId,
              },
            },
          ),
        );
      }
    }
  }

  // --- Validate LOOP_START has at least one outgoing edge ---
  for (const [nodeId, _info] of loopStarts) {
    const outgoingEdges = graph.getOutgoingEdges(nodeId);
    if (outgoingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_START node (${nodeId}) must have at least one outgoing edge`,
          {
            configType: "workflow",
            context: {
              code: "LOOP_START_NO_OUTGOING_EDGES",
              nodeId,
            },
          },
        ),
      );
    }
  }

  // --- Validate LOOP_END has at least one outgoing edge (skip nodes with empty loopId) ---
  for (const [nodeId, info] of loopEnds) {
    if (!info.loopId) continue; // Skip nodes with empty loopId
    const outgoingEdges = graph.getOutgoingEdges(nodeId);
    if (outgoingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_END node (${nodeId}) must have at least one outgoing edge`,
          {
            configType: "workflow",
            context: {
              code: "LOOP_END_NO_OUTGOING_EDGES",
              nodeId,
              loopId: info.loopId,
            },
          },
        ),
      );
    }
  }

  // --- Validate LOOP_START has at least one incoming edge ---
  for (const [nodeId, _info] of loopStarts) {
    const incomingEdges = graph.getIncomingEdges(nodeId);
    if (incomingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `LOOP_START node (${nodeId}) must have at least one incoming edge`,
          {
            configType: "workflow",
            context: {
              code: "LOOP_START_NO_INCOMING_EDGES",
              nodeId,
            },
          },
        ),
      );
    }
  }

  return errors;
}
