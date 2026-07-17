/**
 * START/END Node Validator
 *
 * Validates the topological constraints of START and END nodes:
 * - START node cannot have incoming edges
 * - END node cannot have outgoing edges
 * - Triggered subgraph boundary nodes (START_FROM_TRIGGER, CONTINUE_FROM_TRIGGER) constraints
 *
 * Note: The number and presence of START/END nodes have already been verified in WorkflowValidator.
 * This validator only checks topological constraints.
 */

import type { ID, StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphStructure } from "@wf-agent/types";

/**
 * Validate START and END node constraints for normal workflows
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateStartEndNodes(graph: WorkflowGraphStructure): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Check START node
  if (!graph.startNodeId) {
    errors.push(
      new ConfigurationValidationError("Workflow must contain a START node", {
        configType: "workflow",
        context: {
          code: "MISSING_START_NODE",
        },
      }),
    );
  } else {
    // Check START node in-degree
    const incomingEdges = graph.getIncomingEdges(graph.startNodeId);
    if (incomingEdges.length > 0) {
      errors.push(
        new ConfigurationValidationError("START node cannot have incoming edges", {
          configType: "workflow",
          context: {
            code: "START_NODE_HAS_INCOMING_EDGES",
            nodeId: graph.startNodeId,
          },
        }),
      );
    }
  }

  // Check END node
  if (graph.endNodeIds.size === 0) {
    errors.push(
      new ConfigurationValidationError("Workflow must contain at least one END node", {
        configType: "workflow",
        context: {
          code: "MISSING_END_NODE",
        },
      }),
    );
  } else {
    // Check END node out-degree
    for (const endNodeId of graph.endNodeIds) {
      const outgoingEdges = graph.getOutgoingEdges(endNodeId);
      if (outgoingEdges.length > 0) {
        errors.push(
          new ConfigurationValidationError(`END node (${endNodeId}) cannot have outgoing edges`, {
            configType: "workflow",
            context: {
              code: "END_NODE_HAS_OUTGOING_EDGES",
              nodeId: endNodeId,
            },
          }),
        );
      }
    }
  }

  return errors;
}

/**
 * Validate topological constraints for triggered subgraph boundary nodes
 *
 * Validates:
 * - START_FROM_TRIGGER node cannot have incoming edges
 * - CONTINUE_FROM_TRIGGER node cannot have outgoing edges
 * - Triggered subgraph cannot contain regular START or END nodes
 *
 * Note: The combination of nodes (quantity and presence) has already been verified in WorkflowValidator.
 *
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateTriggeredSubgraphNodes(
  graph: WorkflowGraphStructure,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Check START_FROM_TRIGGER node
  const startFromTriggerNodes: ID[] = [];
  for (const node of graph.nodes.values()) {
    if (node.type === ("START_FROM_TRIGGER" as StaticNodeType)) {
      startFromTriggerNodes.push(node.id);
    }
  }

  if (startFromTriggerNodes.length === 0) {
    errors.push(
      new ConfigurationValidationError(
        "Triggered subworkflow must contain a START_FROM_TRIGGER node",
        {
          configType: "workflow",
          context: {
            code: "MISSING_START_FROM_TRIGGER_NODE",
          },
        },
      ),
    );
  } else if (startFromTriggerNodes.length > 1) {
    errors.push(
      new ConfigurationValidationError(
        "Triggered subworkflow can contain only one START_FROM_TRIGGER node",
        {
          configType: "workflow",
          context: {
            code: "MULTIPLE_START_FROM_TRIGGER_NODES",
          },
        },
      ),
    );
  } else {
    // Check START_FROM_TRIGGER node in-degree
    const startNodeId = startFromTriggerNodes[0]!;
    const incomingEdges = graph.getIncomingEdges(startNodeId);
    if (incomingEdges.length > 0) {
      errors.push(
        new ConfigurationValidationError("START_FROM_TRIGGER node cannot have incoming edges", {
          configType: "workflow",
          context: {
            code: "START_FROM_TRIGGER_NODE_HAS_INCOMING_EDGES",
            nodeId: startNodeId,
          },
        }),
      );
    }
  }

  // Check CONTINUE_FROM_TRIGGER node
  const continueFromTriggerNodes: ID[] = [];
  for (const node of graph.nodes.values()) {
    if (node.type === ("CONTINUE_FROM_TRIGGER" as StaticNodeType)) {
      continueFromTriggerNodes.push(node.id);
    }
  }

  if (continueFromTriggerNodes.length === 0) {
    errors.push(
      new ConfigurationValidationError(
        "Triggered subworkflow must contain a CONTINUE_FROM_TRIGGER node",
        {
          configType: "workflow",
          context: {
            code: "MISSING_CONTINUE_FROM_TRIGGER_NODE",
          },
        },
      ),
    );
  } else if (continueFromTriggerNodes.length > 1) {
    errors.push(
      new ConfigurationValidationError(
        "Triggered subworkflow can contain only one CONTINUE_FROM_TRIGGER node",
        {
          configType: "workflow",
          context: {
            code: "MULTIPLE_CONTINUE_FROM_TRIGGER_NODES",
          },
        },
      ),
    );
  } else {
    // Check CONTINUE_FROM_TRIGGER node out-degree
    const endNodeId = continueFromTriggerNodes[0]!;
    const outgoingEdges = graph.getOutgoingEdges(endNodeId);
    if (outgoingEdges.length > 0) {
      errors.push(
        new ConfigurationValidationError("CONTINUE_FROM_TRIGGER node cannot have outgoing edges", {
          configType: "workflow",
          context: {
            code: "CONTINUE_FROM_TRIGGER_NODE_HAS_OUTGOING_EDGES",
            nodeId: endNodeId,
          },
        }),
      );
    }
  }

  // Check for regular START or END nodes
  for (const node of graph.nodes.values()) {
    if (node.type === ("START" as StaticNodeType)) {
      errors.push(
        new ConfigurationValidationError("Triggered subworkflow cannot contain a START node", {
          configType: "workflow",
          context: {
            code: "TRIGGERED_SUBGRAPH_CONTAINS_START_NODE",
            nodeId: node.id,
          },
        }),
      );
    }
    if (node.type === ("END" as StaticNodeType)) {
      errors.push(
        new ConfigurationValidationError("Triggered subworkflow cannot contain an END node", {
          configType: "workflow",
          context: {
            code: "TRIGGERED_SUBGRAPH_CONTAINS_END_NODE",
            nodeId: node.id,
          },
        }),
      );
    }
  }

  return errors;
}
