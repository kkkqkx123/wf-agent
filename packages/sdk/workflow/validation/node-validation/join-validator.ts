/**
 * Join node validation function
 * Provides static validation logic for the Join node, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { JoinNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../shared/validation/utils.js";

/**
 * Verify Join node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateJoinNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "JOIN");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, JoinNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
