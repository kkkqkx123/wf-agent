/**
 * Start Node Validation Function
 * Provides static validation logic for the Start node, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { StartNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify Start node configuration
 * @param node Static node definition
 * @returns Verification result
 */
export function validateStartNode(node: StaticNode): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "START");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config || {}, StartNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
