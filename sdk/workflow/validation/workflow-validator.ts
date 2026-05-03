/**
 * Workflow Definition Validator
 *
 * Responsibilities:
 * - Verify the data integrity and basic constraints of the workflow definition.
 * - Check the basic fields of nodes and edges, as well as the uniqueness of IDs and the integrity of references.
 * - Verify the schema of node configurations (business logic is not validated).
 * - Verify Hooks, workflow configurations, triggers, etc.
 * - Detect self-referencing issues.
 * - Ensure the matching of workflow types with node types.
 * - Verify the number and presence of START/END nodes.
 * - Verify the combination of nodes that trigger sub-workflows.
 *
 * Differences from GraphValidator:
 * - WorkflowValidator performs validation during the workflow registration phase; the input is a WorkflowTemplate.
 * - GraphValidator performs validation during the graph preprocessing phase; the input is GraphData.
 * - WorkflowValidator validates all rules that can be determined during the definition phase (validation before registration).
 * - GraphValidator validates rules that require the graph structure to be determined (validation during the preprocessing phase).
 *
 * Validation Timing:
 * - Called before the workflow is registered with the WorkflowRegistry.
 * - This is the last line of defense before workflow registration; workflows with obvious defects should not be allowed through.
 *
 * Does Not Include:
 * - Graph topology structure validation (such as cycle detection, reachability analysis, etc.).
 * - Validation of FORK/JOIN pairings and business logic.
 * - Verification of the in-degree and out-degree constraints of START/END nodes.
 * - Verification of node reachability.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import type { Node, NodeHook } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateNodeByType } from "./node-validation/index.js";
import { validateHooks } from "../../core/validation/hook-validator.js";
import { validateTriggers } from "../../core/validation/trigger-validator.js";
import { SelfReferenceValidationStrategy } from "./strategies/self-reference-validation-strategy.js";
import { validateConfig } from "../../core/validation/utils.js";
import { WorkflowConfigSchema, WorkflowTemplateBasicSchema } from "@wf-agent/types";

/**
 * Workflow Validator Class
 */
export class WorkflowValidator {
  /**
   * Verify workflow definition
   * @param workflow: The workflow definition
   * @returns: The verification result
   */
  validate(
    workflow: WorkflowTemplate,
  ): Result<WorkflowTemplate, ConfigurationValidationError[]> {
    const errors: ConfigurationValidationError[] = [];

    // Verify basic information
    errors.push(...this.validateBasicInfo(workflow));

    // Verify workflow type
    errors.push(...this.validateWorkflowType(workflow));

    // Verify the node.
    errors.push(...this.validateNodes(workflow));

    // Verify the edge
    errors.push(...this.validateEdges(workflow));

    // Verify reference integrity
    errors.push(...this.validateReferences(workflow));

    // Verify the configuration.
    errors.push(...this.validateConfig(workflow));

    // Verify the trigger
    errors.push(...this.validateTriggers(workflow));

    // Verify self-reference
    errors.push(...this.validateSelfReferences(workflow));

    // Verify tool configuration
    errors.push(...this.validateTools(workflow));

    if (errors.length > 0) {
      return err(errors);
    }
    return ok(workflow);
  }

  /**
   * Verify basic information
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateBasicInfo(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const result = validateConfig(workflow, WorkflowTemplateBasicSchema, "workflow", "workflow");
    if (result.isErr()) {
      return result.error;
    }
    return [];
  }

  /**
   * Verify the matching of workflow types with node types
   *
   * Validation rules:
   * - TRIGGERED_SUBWORKFLOW: Must contain START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER, and should not contain SUBGRAPH.
   * - STANDALONE: Should not contain SUBGRAPH nodes or EXECUTE_TRIGGERED_SUBGRAPH triggers.
   * - DEPENDENT: Must contain SUBGRAPH nodes or EXECUTE_TRIGGERED_SUBGRAPH triggers.
   *
   * Note: Detailed verification of the number and presence of nodes is performed in GraphValidator.
   * This method only verifies the matching relationship between workflow types and node types.
   *
   * @param workflow: Workflow definition
   * @returns: Validation result
   */
  private validateWorkflowType(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];
    const { type, nodes, triggers } = workflow;

    // Check if it contains special nodes.
    const hasStartFromTrigger = nodes.some(node => node.type === "START_FROM_TRIGGER");
    const hasContinueFromTrigger = nodes.some(node => node.type === "CONTINUE_FROM_TRIGGER");
    const hasSubgraphNode = nodes.some(node => node.type === "SUBGRAPH");

    // Check if the EXECUTE_TRIGGERED_SUBGRAPH trigger is present.
    const hasExecuteTriggeredSubgraphTrigger =
      triggers?.some(trigger => {
        if ("action" in trigger) {
          return trigger.action.type === "execute_triggered_subgraph";
        }
        return false;
      }) || false;

    // Verify the workflow structure based on the type of declaration.
    switch (type) {
      case "TRIGGERED_SUBWORKFLOW":
        // The trigger workflow must include START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER.
        if (!hasStartFromTrigger) {
          errors.push(
            new ConfigurationValidationError(
              "Triggered subworkflow must contain START_FROM_TRIGGER node",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        if (!hasContinueFromTrigger) {
          errors.push(
            new ConfigurationValidationError(
              "Triggered subworkflow must contain CONTINUE_FROM_TRIGGER node",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        // The trigger workflow should not contain SUBGRAPH nodes.
        if (hasSubgraphNode) {
          errors.push(
            new ConfigurationValidationError(
              "Triggered subworkflow should not contain SUBGRAPH node",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        break;

      case "STANDALONE":
        // Independent workflows should not contain SUBGRAPH nodes or EXECUTE_TRIGGERED_SUBGRAPH triggers.
        if (hasSubgraphNode) {
          errors.push(
            new ConfigurationValidationError(
              "Standalone workflow should not contain SUBGRAPH node. Use DEPENDENT type instead.",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        if (hasExecuteTriggeredSubgraphTrigger) {
          errors.push(
            new ConfigurationValidationError(
              "Standalone workflow should not contain EXECUTE_TRIGGERED_SUBGRAPH trigger. Use DEPENDENT type instead.",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        break;

      case "DEPENDENT":
        // The dependency workflow must include a SUBGRAPH node or an EXECUTE_TRIGGERED_SUBGRAPH trigger.
        if (!hasSubgraphNode && !hasExecuteTriggeredSubgraphTrigger) {
          errors.push(
            new ConfigurationValidationError(
              "Dependent workflow must contain either SUBGRAPH node or EXECUTE_TRIGGERED_SUBGRAPH trigger",
              {
                configType: "workflow",
                configPath: "workflow.type",
              },
            ),
          );
        }
        break;

      default:
        errors.push(
          new ConfigurationValidationError(`Invalid workflow type: ${type}`, {
            configType: "workflow",
            configPath: "workflow.type",
          }),
        );
    }

    return errors;
  }

  /**
   * Verify Node
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateNodes(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Verify that the node array is not empty.
    if (!workflow.nodes || workflow.nodes.length === 0) {
      errors.push(
        new ConfigurationValidationError("Workflow must have at least one node", {
          configType: "workflow",
          configPath: "workflow.nodes",
        }),
      );
      return errors;
    }

    // Verify the uniqueness of node IDs.
    const nodeIds = new Set<string>();
    const startNodes: Node[] = [];
    const endNodes: Node[] = [];
    const startFromTriggerNodes: Node[] = [];
    const continueFromTriggerNodes: Node[] = [];

    for (let i = 0; i < workflow.nodes.length; i++) {
      const node = workflow.nodes[i];
      if (!node) continue;

      const path = `workflow.nodes[${i}]`;

      // Verify the basic fields of the node.
      if (!node.id || node.id === "") {
        errors.push(
          new ConfigurationValidationError("Node ID is required", {
            configType: "node",
            configPath: `${path}.id`,
          }),
        );
      }
      if (!node.name || node.name === "") {
        errors.push(
          new ConfigurationValidationError("Node name is required", {
            configType: "node",
            configPath: `${path}.name`,
          }),
        );
      }
      if (!node.type) {
        errors.push(
          new ConfigurationValidationError("Node type is required", {
            configType: "node",
            configPath: `${path}.type`,
          }),
        );
      }

      // Check the uniqueness of node IDs.
      if (node.id && nodeIds.has(node.id)) {
        errors.push(
          new ConfigurationValidationError(`Node ID must be unique: ${node.id}`, {
            configType: "node",
            configPath: `${path}.id`,
          }),
        );
      }
      if (node.id) {
        nodeIds.add(node.id);
      }

      // Verify node configuration (using the node validation function)
      if (node.id && node.type) {
        const configResult = validateNodeByType(node);
        if (configResult.isErr()) {
          errors.push(...configResult.error);
        }
      }

      // Verify node Hooks
      if (node.id && node.hooks && node.hooks.length > 0) {
        const hooksResult = validateHooks(node.hooks as NodeHook[], node.id);
        if (hooksResult.isErr()) {
          errors.push(...hooksResult.error);
        }
      }

      // Count special node types
      if (node.type === "START") {
        startNodes.push(node);
      } else if (node.type === "END") {
        endNodes.push(node);
      } else if (node.type === "START_FROM_TRIGGER") {
        startFromTriggerNodes.push(node);
      } else if (node.type === "CONTINUE_FROM_TRIGGER") {
        continueFromTriggerNodes.push(node);
      }
    }

    // Business rules for verifying node types
    const hasStartFromTrigger = startFromTriggerNodes.length > 0;
    const hasContinueFromTrigger = continueFromTriggerNodes.length > 0;

    if (hasStartFromTrigger || hasContinueFromTrigger) {
      // Trigger workflow: Must include START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER, and must not include START and END.
      if (!hasStartFromTrigger) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subgraph must have exactly one START_FROM_TRIGGER node",
            {
              configType: "workflow",
              configPath: "workflow.nodes",
            },
          ),
        );
      } else if (startFromTriggerNodes.length > 1) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subgraph must have exactly one START_FROM_TRIGGER node",
            {
              configType: "workflow",
              configPath: "workflow.nodes",
            },
          ),
        );
      }

      if (!hasContinueFromTrigger) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subgraph must have exactly one CONTINUE_FROM_TRIGGER node",
            {
              configType: "workflow",
              configPath: "workflow.nodes",
            },
          ),
        );
      } else if (continueFromTriggerNodes.length > 1) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subgraph must have exactly one CONTINUE_FROM_TRIGGER node",
            {
              configType: "workflow",
              configPath: "workflow.nodes",
            },
          ),
        );
      }

      if (startNodes.length > 0) {
        errors.push(
          new ConfigurationValidationError("Triggered subgraph cannot contain START node", {
            configType: "workflow",
            configPath: "workflow.nodes",
          }),
        );
      }

      if (endNodes.length > 0) {
        errors.push(
          new ConfigurationValidationError("Triggered subgraph cannot contain END node", {
            configType: "workflow",
            configPath: "workflow.nodes",
          }),
        );
      }
    } else {
      // Regular workflow: Must include START and END nodes.
      if (startNodes.length === 0) {
        errors.push(
          new ConfigurationValidationError("Workflow must have exactly one START node", {
            configType: "workflow",
            configPath: "workflow.nodes",
          }),
        );
      } else if (startNodes.length > 1) {
        errors.push(
          new ConfigurationValidationError("Workflow must have exactly one START node", {
            configType: "workflow",
            configPath: "workflow.nodes",
          }),
        );
      }

      if (endNodes.length === 0) {
        errors.push(
          new ConfigurationValidationError("Workflow must have at least one END node", {
            configType: "workflow",
            configPath: "workflow.nodes",
          }),
        );
      }
    }

    return errors;
  }

  /**
   * Verify the workflow
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateEdges(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];
    const edgeIds = new Set<string>();

    for (let i = 0; i < workflow.edges.length; i++) {
      const edge = workflow.edges[i];
      if (!edge) continue;

      const path = `workflow.edges[${i}]`;

      // Check the uniqueness of edge IDs.
      if (edgeIds.has(edge.id)) {
        errors.push(
          new ConfigurationValidationError(`Edge ID must be unique: ${edge.id}`, {
            configType: "edge",
            configPath: `${path}.id`,
          }),
        );
      }
      edgeIds.add(edge.id);

      // Check the basic information of the edge.
      if (!edge.id) {
        errors.push(
          new ConfigurationValidationError("Edge ID is required", {
            configType: "edge",
            configPath: `${path}.id`,
          }),
        );
      }

      if (!edge.sourceNodeId) {
        errors.push(
          new ConfigurationValidationError("Edge source node ID is required", {
            configType: "edge",
            configPath: `${path}.sourceNodeId`,
          }),
        );
      }

      if (!edge.targetNodeId) {
        errors.push(
          new ConfigurationValidationError("Edge target node ID is required", {
            configType: "edge",
            configPath: `${path}.targetNodeId`,
          }),
        );
      }

      if (!edge.type) {
        errors.push(
          new ConfigurationValidationError("Edge type is required", {
            configType: "edge",
            configPath: `${path}.type`,
          }),
        );
      }
    }

    return errors;
  }

  /**
   * Verify reference integrity
   * Check if the referenced nodes exist
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateReferences(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];
    const nodeIds = new Set(workflow.nodes.map(n => n.id));

    // Check the node references of the edges.
    for (const edge of workflow.edges) {
      if (!nodeIds.has(edge.sourceNodeId)) {
        errors.push(
          new ConfigurationValidationError(`Edge source node not found: ${edge.sourceNodeId}`, {
            configType: "edge",
            configPath: `workflow.edges[${edge.id}].sourceNodeId`,
          }),
        );
      }
      if (!nodeIds.has(edge.targetNodeId)) {
        errors.push(
          new ConfigurationValidationError(`Edge target node not found: ${edge.targetNodeId}`, {
            configType: "edge",
            configPath: `workflow.edges[${edge.id}].targetNodeId`,
          }),
        );
      }
    }

    return errors;
  }

  /**
   * Verify Configuration
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateConfig(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    if (!workflow.config) {
      return [];
    }

    const result = validateConfig(
      workflow.config,
      WorkflowConfigSchema,
      "workflow.config",
      "workflow",
    );
    if (result.isErr()) {
      return result.error;
    }
    return [];
  }

  /**
   * Verify trigger
   * @param workflow Workflow definition
   * @returns Verification result
   */
  private validateTriggers(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // If no trigger is found, return a success status directly.
    if (!workflow.triggers || workflow.triggers.length === 0) {
      return [];
    }

    // Verify trigger configuration
    const triggersResult = validateTriggers(workflow.triggers, "workflow.triggers");
    if (triggersResult.isErr()) {
      errors.push(...triggersResult.error);
    }

    return errors;
  }

  /**
   * Verify self-reference
   * Use the Strategy pattern to detect self-references in SUBGRAPH and START_FROM_TRIGGER nodes
   * @param workflow Workflow definition
   * @returns Verification result
   */
  private validateSelfReferences(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors = SelfReferenceValidationStrategy.validateNodes(workflow.nodes, workflow.id);

    return errors;
  }

  /**
   * Verify tool configuration
   * @param workflow: Workflow definition
   * @returns: Verification result
   */
  private validateTools(workflow: WorkflowTemplate): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Verify the availableTools configuration.
    if (workflow.availableTools) {
      if (!Array.isArray(workflow.availableTools.initial)) {
        errors.push(
          new ConfigurationValidationError("availableTools.initial must be an array of tool IDs", {
            configType: "workflow",
            configPath: "workflow.availableTools.initial",
          }),
        );
      }
    }

    return errors;
  }
}
