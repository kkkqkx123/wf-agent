/**
 * LoopEnd Node Validation Function
 * Provides static validation logic for the LoopEnd node, using zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { LoopEndNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LoopEnd node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateLoopEndNode(node: Node): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "LOOP_END");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, LoopEndNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
