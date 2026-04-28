/**
 * LLM Node Validation Function
 * Provides static validation logic for LLM nodes, using zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { LLMNodeConfigSchema } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LLM node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateLLMNode(node: Node): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "LLM");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, LLMNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
