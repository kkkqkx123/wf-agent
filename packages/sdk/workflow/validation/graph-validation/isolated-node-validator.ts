/**
 * Isolated Node Validator
 *
 * Detects nodes that have no incoming or outgoing edges.
 * Boundary nodes (START, END, START_FROM_TRIGGER, CONTINUE_FROM_TRIGGER) are excluded.
 */

import type { StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphStructure } from "@wf-agent/types";

/**
 * Validate isolated nodes in the graph
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateIsolatedNodes(graph: WorkflowGraphStructure): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  for (const node of graph.nodes.values()) {
    const incomingEdges = graph.getIncomingEdges(node.id);
    const outgoingEdges = graph.getOutgoingEdges(node.id);

    // Exclude boundary nodes
    if (
      node.type === ("START" as StaticNodeType) ||
      node.type === ("END" as StaticNodeType) ||
      node.type === ("START_FROM_TRIGGER" as StaticNodeType) ||
      node.type === ("CONTINUE_FROM_TRIGGER" as StaticNodeType)
    ) {
      continue;
    }

    if (incomingEdges.length === 0 && outgoingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `Node (${node.id}) is isolated, has no incoming or outgoing edges`,
          {
            configType: "workflow",
            context: {
              code: "ISOLATED_NODE",
              nodeId: node.id,
            },
          },
        ),
      );
    }
  }

  return errors;
}
