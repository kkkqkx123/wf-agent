/**
 * Self-reference verification strategy
 * Provides workflow-level self-reference detection logic
 * Used to verify that SUBGRAPH nodes cannot reference their own workflow
 *
 * Note: The START_FROM_TRIGGER node is now empty and no longer contains the subgraphId.
 * The triggered sub-workflow is specified through the triggeredWorkflowId in the ExecuteTriggeredSubgraphActionConfig of the trigger.
 */

import type { Node } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

/**
 * Sub-workflow configuration interface
 */
export interface SubgraphConfig {
  /** Sub-workflow ID */
  subgraphId: string;
}

/**
 * Self-reference validation strategy
 * Detects whether a node references the workflow in which it is located.
 */
export class SelfReferenceValidationStrategy {
  /**
   * Check if the node is of the subtype workflow node.
   * @param node: Node definition
   * @returns: Whether it is a subtype workflow node
   */
  static isSubgraphNode(node: Node): boolean {
    // The START_FROM_TRIGGER node is now empty and no longer references any other workflows.
    return node.type === "SUBGRAPH";
  }

  /**
   * Verify if the node contains self-references
   * @param node Node definition
   * @param workflowId Current workflow ID
   * @param path Prefix for error paths
   * @returns List of verification errors
   */
  static validate(node: Node, workflowId: string, path: string): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Verify only SUBGRAPH nodes.
    if (node.type !== "SUBGRAPH") {
      return errors;
    }

    const config = node.config as SubgraphConfig;

    // Check if it references its own workflow.
    if (config && config.subgraphId === workflowId) {
      errors.push(
        new ConfigurationValidationError(
          "Child workflow nodes cannot reference their own workflows",
          {
            configType: "node",
            configPath: `${path}.config.subgraphId`,
            value: config.subgraphId,
            context: { code: "SELF_REFERENCE", nodeId: node.id, subgraphId: config.subgraphId },
          },
        ),
      );
    }

    return errors;
  }

  /**
   * Batch verification of self-references for multiple nodes
   * @param nodes Array of nodes
   * @param workflowId Current workflow ID
   * @param pathPrefix Path prefix
   * @returns List of verification errors
   */
  static validateNodes(
    nodes: Node[],
    workflowId: string,
    pathPrefix: string = "workflow.nodes",
  ): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    for (let i = 0; i < nodes.length; i++) {
      const node = nodes[i];
      if (!node) continue;

      const path = `${pathPrefix}[${i}]`;
      const nodeErrors = this.validate(node, workflowId, path);
      errors.push(...nodeErrors);
    }

    return errors;
  }
}
