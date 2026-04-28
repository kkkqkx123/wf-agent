/**
 * User Interaction Node Validation Function
 * Provides static validation logic for user interaction nodes, using Zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { UserInteractionNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify user interaction node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateUserInteractionNode(
  node: Node,
): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "USER_INTERACTION");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, UserInteractionNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
