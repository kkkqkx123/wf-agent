/**
 * Node Validator
 * Responsible for verifying the business rules related to node configuration
 *
 * Design Notes:
 * - TypeScript's ability to recognize union types provides compile-time type checking.
 * - This validator focuses on runtime business rule validation (external inputs, business constraints).
 * - It avoids re-verifying content that can already be guaranteed by the type system.
 */

import type { Node } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { err } from "@wf-agent/common-utils";
import { validateNodeByType } from "./node-validation/index.js";

/**
 * Node Validator Class
 *
 * Validation Scope:
 * 1. The basic structure of external input data (from JSON/YAML/API)
 * 2. Business rule constraints (e.g., the forkPaths of a FORK node must not be empty)
 * 3. Cross-field association validation
 *
 * Not Validated:
 * - The matching of type and config (ensured during TypeScript compilation)
 * - The correctness of field types (ensured during TypeScript compilation)
 */
export class NodeValidator {
  /**
   * Verify node
   * @param node The node to be verified
   * @returns The verification result
   */
  validateNode(node: Node): Result<Node, ConfigurationValidationError[]> {
    // Verify business rules
    return this.validateBusinessRules(node);
  }

  /**
   * Verify business rules
   * @param node Node
   * @returns Verification result
   */
  private validateBusinessRules(node: Node): Result<Node, ConfigurationValidationError[]> {
    // Call business rule validations for each node type.
    return validateNodeByType(node);
  }

  /**
   * Batch node validation
   * @param nodes: Array of nodes
   * @returns: Array of validation results
   */
  validateNodes(nodes: Node[]): Result<Node, ConfigurationValidationError[]>[] {
    return nodes.map(node => this.validateNode(node));
  }

  /**
   * Verify external input node data (from JSON/API sources)
   * Used to validate the raw data received from external sources and ensure its basic structure is correct.
   *
   * @param rawNode: Raw node data
   * @returns: Validation result
   */
  validateRawNode(rawNode: unknown): Result<Node, ConfigurationValidationError[]> {
    // Basic structure check
    if (!rawNode || typeof rawNode !== "object") {
      return err([
        new ConfigurationValidationError("Node must be an object", {
          configType: "node",
        }),
      ]);
    }

    const node = rawNode as Record<string, unknown>;

    // Required field validation - Using parentheses to access fields prevents index signature issues.
    const nodeId = node["id"];
    const nodeName = node["name"];
    const nodeType = node["type"];
    const nodeConfig = node["config"];
    const nodeOutgoingEdgeIds = node["outgoingEdgeIds"];
    const nodeIncomingEdgeIds = node["incomingEdgeIds"];

    if (typeof nodeId !== "string" || nodeId.length === 0) {
      return err([
        new ConfigurationValidationError("Node id is required and must be a non-empty string", {
          configType: "node",
          configPath: "id",
        }),
      ]);
    }

    if (typeof nodeName !== "string" || nodeName.length === 0) {
      return err([
        new ConfigurationValidationError("Node name is required and must be a non-empty string", {
          configType: "node",
          configPath: "name",
        }),
      ]);
    }

    // Type field validation
    const validTypes = [
      "START",
      "END",
      "VARIABLE",
      "FORK",
      "JOIN",
      "SUBGRAPH",
      "SCRIPT",
      "LLM",
      "ADD_TOOL",
      "USER_INTERACTION",
      "ROUTE",
      "CONTEXT_PROCESSOR",
      "LOOP_START",
      "LOOP_END",
      "AGENT_LOOP",
      "START_FROM_TRIGGER",
      "CONTINUE_FROM_TRIGGER",
    ];

    if (typeof nodeType !== "string" || !validTypes.includes(nodeType)) {
      return err([
        new ConfigurationValidationError(`Invalid node type: ${nodeType}`, {
          configType: "node",
          configPath: "type",
        }),
      ]);
    }

    // Config field validation
    if (nodeConfig === undefined || nodeConfig === null) {
      return err([
        new ConfigurationValidationError("Node config is required", {
          configType: "node",
          configPath: "config",
        }),
      ]);
    }

    // Array field validation
    if (nodeOutgoingEdgeIds !== undefined && !Array.isArray(nodeOutgoingEdgeIds)) {
      return err([
        new ConfigurationValidationError("outgoingEdgeIds must be an array", {
          configType: "node",
          configPath: "outgoingEdgeIds",
        }),
      ]);
    }

    if (nodeIncomingEdgeIds !== undefined && !Array.isArray(nodeIncomingEdgeIds)) {
      return err([
        new ConfigurationValidationError("incomingEdgeIds must be an array", {
          configType: "node",
          configPath: "incomingEdgeIds",
        }),
      ]);
    }

    // After passing the basic checks, proceed with the validation of business rules.
    return this.validateNode(node as unknown as Node);
  }
}
