/**
 * LoopStart node validation function
 * Provides static validation logic for the LoopStart node, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { LoopStartNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LoopStart node configuration
 * @param node: StaticNode definition
 * @returns: Verification result
 */
export function validateLoopStartNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "LOOP_START");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, LoopStartNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
