/**
 * WorkflowValidator - Workflow Definition Validator
 *
 * Validates workflow definitions during the builder phase.
 * Extracted from WorkflowBuilder to keep the builder focused on construction.
 */

import type {
  StaticNode,
  Edge,
  VariableDefinition,
  WorkflowTrigger,
  TriggerReference,
  SubgraphNodeConfig,
  WorkflowStartConfig,
  LoopStartNodeConfig,
} from "@wf-agent/types";
import type { TriggerTemplateRegistry } from "../../../shared/registry/trigger-template-registry.js";

/**
 * Validation input data
 */
export interface WorkflowValidationInput {
  nodes: Map<string, StaticNode>;
  edges: Edge[];
  variables: VariableDefinition[];
  triggers: (WorkflowTrigger | TriggerReference)[];
  triggerTemplateRegistry: TriggerTemplateRegistry;
}

/**
 * WorkflowValidator - validates workflow definitions during build phase
 */
export class WorkflowValidator {
  private input: WorkflowValidationInput;

  constructor(input: WorkflowValidationInput) {
    this.input = input;
  }

  /**
   * Run validation against the workflow definition.
   * Throws an Error with all validation errors concatenated.
   */
  validate(): void {
    const errors: string[] = [];

    this.validateNodes(errors);
    this.validateEdges(errors);
    this.validateTriggers(errors);
    this.validateNodeConfigs(errors);

    if (errors.length > 0) {
      throw new Error(`Workflow validation failed:\n${errors.join("\n")}`);
    }
  }

  /**
   * Run validation and return errors array without throwing.
   * Useful when the caller wants to collect errors without an exception.
   */
  validateSilently(): string[] {
    const errors: string[] = [];
    this.validateNodes(errors);
    this.validateEdges(errors);
    this.validateTriggers(errors);
    this.validateNodeConfigs(errors);
    return errors;
  }

  private validateNodes(errors: string[]): void {
    const { nodes } = this.input;

    // Check for nodes
    if (nodes.size === 0) {
      errors.push("The workflow must have at least one node.");
    }

    // Check for duplicate node IDs
    const nodeIds = Array.from(nodes.keys());
    if (nodeIds.length !== new Set(nodeIds).size) {
      errors.push("Node IDs must be unique within the workflow.");
    }

    // Check for START nodes
    const startNodes = Array.from(nodes.values()).filter(n => n.type === "START");
    if (startNodes.length === 0) {
      errors.push("The workflow must have a START node.");
    } else if (startNodes.length > 1) {
      errors.push("The workflow can only have one START node.");
    }

    // Check for END nodes
    const endNodes = Array.from(nodes.values()).filter(n => n.type === "END");
    if (endNodes.length === 0) {
      errors.push("The workflow must have an END node.");
    } else if (endNodes.length > 1) {
      errors.push("The workflow can only have one END node.");
    }
  }

  private validateEdges(errors: string[]): void {
    const { nodes, edges } = this.input;

    for (const edge of edges) {
      if (!nodes.has(edge.sourceNodeId)) {
        errors.push(`Source node of edge does not exist: ${edge.sourceNodeId}`);
      }
      if (!nodes.has(edge.targetNodeId)) {
        errors.push(`Target node of edge does not exist: ${edge.targetNodeId}`);
      }
    }
  }

  private validateTriggers(errors: string[]): void {
    const { triggers, triggerTemplateRegistry } = this.input;

    for (const trigger of triggers) {
      if ("templateName" in trigger) {
        const reference = trigger as TriggerReference;
        if (!triggerTemplateRegistry.has(reference.templateName)) {
          errors.push(`Trigger template '${reference.templateName}' does not exist`);
        }
      }
    }
  }

  private validateNodeConfigs(errors: string[]): void {
    const { nodes, variables } = this.input;

    for (const node of nodes.values()) {
      if (node.type === "SUBGRAPH") {
        const config = node.config as SubgraphNodeConfig;

        if (config.variableInputs && config.variableInputs.length > 0) {
          for (const input of config.variableInputs) {
            const parentVar = variables.find(v => v.name === input.externalName);
            if (!parentVar && input.required && input.defaultValue === undefined) {
              errors.push(
                `Subgraph '${node.id}' requires variable '${input.externalName}' which is not defined in parent workflow`,
              );
            }
          }
        }
      }

      if (node.type === "START") {
        const config = node.config as WorkflowStartConfig;

        if (config.variableInputs) {
          const internalNames = config.variableInputs.map(i => i.internalName);
          const uniqueNames = new Set(internalNames);
          if (uniqueNames.size !== internalNames.length) {
            errors.push(`START node has duplicate internal variable names`);
          }
        }
      }

      if (node.type === "LOOP_START") {
        const config = node.config as LoopStartNodeConfig;

        if (config.variableInputs && config.variableInputs.length > 0) {
          for (const input of config.variableInputs) {
            const parentVar = variables.find(v => v.name === input.externalName);
            if (!parentVar && input.required && input.defaultValue === undefined) {
              errors.push(
                `Loop '${config.loopId}' requires variable '${input.externalName}' which is not defined in parent workflow`,
              );
            }
          }
        }
      }
    }
  }
}