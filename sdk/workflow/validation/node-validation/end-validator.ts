/**
 * End Node Validation Function
 * Provides static validation logic for the End node.
 *
 * Note: END nodes use WorkflowEndConfig which supports explicit variable
 * and message context output mapping (like return values).
 */

import type { StaticNode } from "@wf-agent/types";
import { WorkflowEndConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify End node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateEndNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "END");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config || {}, WorkflowEndConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
