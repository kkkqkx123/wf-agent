/**
 * Variable node validation function
 * Provides static validation logic for Variable nodes, using zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { VariableNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify Variable node configuration
 * @param node: Node definition
 * @returns: Verification result
 */
export function validateVariableNode(node: Node): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "VARIABLE");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, VariableNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
