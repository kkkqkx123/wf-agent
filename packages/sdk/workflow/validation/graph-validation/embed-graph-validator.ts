/**
 * EMBED_GRAPH Validator
 *
 * Validates EMBED_GRAPH nodes:
 * - Existence of embedId configuration
 * - Constraint validation (no variable mappings allowed)
 *
 * Note: Full constraint validation (checking if referenced workflows have variables/triggers)
 * requires access to the workflow registry and is done in WorkflowGraphBuilder.processSubgraphs.
 */

import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphStructure } from "../../entities/workflow-graph-structure.js";

/**
 * Validate EMBED_GRAPH node existence and configuration
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateEmbedGraphExistence(
  graph: WorkflowGraphStructure,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  for (const node of graph.nodes.values()) {
    const originalType = node.originalNode?.type;
    if (originalType === "EMBED_GRAPH") {
      const embedConfig = node.originalNode?.config as { embedId?: string } | undefined;
      if (!embedConfig || !embedConfig.embedId) {
        errors.push(
          new ConfigurationValidationError(
            `EMBED_GRAPH node (${node.id}) is missing embedId configuration`,
            {
              configType: "workflow",
              context: {
                code: "MISSING_EMBED_ID",
                nodeId: node.id,
              },
            },
          ),
        );
      }
    }
  }

  return errors;
}

/**
 * Validate EMBED_GRAPH constraints
 *
 * Validates that embedded workflows referenced by EMBED_GRAPH nodes meet the constraints:
 * - Rule 1: Cannot define variables
 * - Rule 2: Cannot have triggers
 * - Rule 3: Cannot contain VARIABLE nodes
 *
 * Note: This validation is performed at the graph level after graph building.
 * The actual constraint checking requires access to the workflow registry.
 *
 * This method validates what can be checked at the graph level:
 * - EMBED_GRAPH nodes should not have variable mappings (they don't support them)
 *
 * @param graph Graph data containing EMBED_GRAPH nodes
 * @returns List of validation errors
 */
export function validateEmbedGraphConstraints(
  graph: WorkflowGraphStructure,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Note: Full constraint validation requires access to the workflow registry
  // to check if the referenced workflows have variables/triggers.
  // This is currently done in WorkflowGraphBuilder.processSubgraphs.
  //
  // This method validates what can be checked at the graph level:
  // - EMBED_GRAPH nodes should not have variable mappings (they don't support them)

  for (const node of graph.nodes.values()) {
    const originalType = node.originalNode?.type;
    if (originalType === "EMBED_GRAPH") {
      const config = node.originalNode?.config as
        | {
            variableInputs?: unknown[];
            variableOutputs?: unknown[];
          }
        | undefined;

      // EMBED_GRAPH should not have variable mappings
      if (config?.variableInputs && config.variableInputs.length > 0) {
        errors.push(
          new ConfigurationValidationError(
            `EMBED_GRAPH node '${node.id}' should not have variableInputs. Use SUBGRAPH for variable passing.`,
            {
              configType: "workflow",
              context: {
                code: "EMBED_GRAPH_HAS_VARIABLE_INPUTS",
                nodeId: node.id,
              },
            },
          ),
        );
      }

      if (config?.variableOutputs && config.variableOutputs.length > 0) {
        errors.push(
          new ConfigurationValidationError(
            `EMBED_GRAPH node '${node.id}' should not have variableOutputs. Use SUBGRAPH for variable passing.`,
            {
              configType: "workflow",
              context: {
                code: "EMBED_GRAPH_HAS_VARIABLE_OUTPUTS",
                nodeId: node.id,
              },
            },
          ),
        );
      }
    }
  }

  return errors;
}
