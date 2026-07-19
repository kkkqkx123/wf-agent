/**
 * Subgraph Validator
 *
 * Validates SUBGRAPH nodes:
 * - Existence of subgraphId configuration
 * - Variable mapping format validation (variableInputs and variableOutputs)
 * - Interface compatibility checks
 *
 * Note: Detailed runtime validation (actual variable existence) happens during execution.
 */

import type { StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphStructure } from "@wf-agent/types";

/**
 * Validate subgraph existence
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateSubgraphExistence(
  graph: WorkflowGraphStructure,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  for (const node of graph.nodes.values()) {
    if (node.type === ("SUBGRAPH" as StaticNodeType)) {
      const _subgraphConfig = node.originalNode?.config as { subgraphId?: string } | undefined;
      if (!_subgraphConfig || !_subgraphConfig.subgraphId) {
        errors.push(
          new ConfigurationValidationError(
            `SUBGRAPH node (${node.id}) is missing subgraphId configuration`,
            {
              configType: "workflow",
              context: {
                code: "MISSING_SUBGRAPH_ID",
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
 * Validate subgraph interface compatibility
 *
 * Validates that SUBGRAPH nodes have compatible variable mappings:
 * - variableInputs reference valid parent workflow variables or are optional with defaults
 * - variableOutputs reference valid subworkflow variables
 *
 * Note: This validation is performed at the graph level, checking all SUBGRAPH nodes.
 * Detailed runtime validation (actual variable existence) happens during execution.
 *
 * @param graph Graph data containing SUBGRAPH nodes
 * @returns List of validation errors
 */
export function validateSubgraphCompatibility(
  graph: WorkflowGraphStructure,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Get all SUBGRAPH nodes
  const subgraphNodes: Array<{
    nodeId: string;
    subworkflowId: string;
    variableInputs?: Array<{
      sourcePath: string;
      internalName: string;
      required?: boolean;
      defaultValue?: unknown;
    }>;
    variableOutputs?: Array<{
      internalName: string;
      targetPath: string;
    }>;
    dataInputs?: Array<{
      parentField: string;
      internalName: string;
      required?: boolean;
      defaultValue?: unknown;
    }>;
  }> = [];

  for (const node of graph.nodes.values()) {
    if (node.type === ("SUBGRAPH" as StaticNodeType)) {
      const config = node.originalNode?.config as
        | {
            subgraphId?: string;
            variableInputs?: Array<{
              sourcePath: string;
              internalName: string;
              required?: boolean;
              defaultValue?: unknown;
            }>;
            variableOutputs?: Array<{
              internalName: string;
              targetPath: string;
            }>;
            dataInputs?: Array<{
              parentField: string;
              internalName: string;
              required?: boolean;
              defaultValue?: unknown;
            }>;
          }
        | undefined;

      if (config?.subgraphId) {
        subgraphNodes.push({
          nodeId: node.id,
          subworkflowId: config.subgraphId,
          variableInputs: config.variableInputs,
          variableOutputs: config.variableOutputs,
          dataInputs: config.dataInputs,
        });
      }
    }
  }

  // Validate each SUBGRAPH node's variable mappings
  for (const subgraphNode of subgraphNodes) {
    // Check variableInputs format
    if (subgraphNode.variableInputs && subgraphNode.variableInputs.length > 0) {
      for (const input of subgraphNode.variableInputs) {
        if (!input.sourcePath || !input.sourcePath.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has variableInput with missing sourcePath`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_SOURCE_PATH",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                  internalName: input.internalName,
                },
              },
            ),
          );
        }

        if (!input.internalName || !input.internalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has variableInput with missing internalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_INTERNAL_NAME",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                },
              },
            ),
          );
        }

        // Warn if required input has no default value (runtime validation will catch missing values)
        if (input.required && input.defaultValue === undefined) {
          // This is not an error, just a potential runtime issue
          // We could add a warning log here if needed
        }
      }
    }

    // Check variableOutputs format
    if (subgraphNode.variableOutputs && subgraphNode.variableOutputs.length > 0) {
      for (const output of subgraphNode.variableOutputs) {
        if (!output.internalName || !output.internalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has variableOutput with missing internalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_OUTPUT_INTERNAL_NAME",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                  targetPath: output.targetPath,
                },
              },
            ),
          );
        }

        if (!output.targetPath || !output.targetPath.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has variableOutput with missing targetPath`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_OUTPUT_TARGET_PATH",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                  internalName: output.internalName,
                },
              },
            ),
          );
        }
      }
    }

    // Check dataInputs format
    if (subgraphNode.dataInputs && subgraphNode.dataInputs.length > 0) {
      for (const dataInput of subgraphNode.dataInputs) {
        if (!dataInput.parentField || !dataInput.parentField.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has dataInput with missing parentField`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_DATA_INPUT_PARENT_FIELD",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                  internalName: dataInput.internalName,
                },
              },
            ),
          );
        }

        if (!dataInput.internalName || !dataInput.internalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SUBGRAPH node '${subgraphNode.nodeId}' has dataInput with missing internalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_DATA_INPUT_INTERNAL_NAME",
                  nodeId: subgraphNode.nodeId,
                  subworkflowId: subgraphNode.subworkflowId,
                  parentField: dataInput.parentField,
                },
              },
            ),
          );
        }
      }
    }
  }

  return errors;
}
