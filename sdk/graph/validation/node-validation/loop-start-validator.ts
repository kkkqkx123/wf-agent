/**
 * LoopStart node validation function
 * Provides static validation logic for the LoopStart node, using zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { LoopStartNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LoopStart node configuration
 * @param node: Node definition
 * @returns: Verification result
 */
export function validateLoopStartNode(node: Node): Result<Node, ConfigurationValidationError[]> {
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
