/**
 * ContextProcessor node validation function
 * Provides static validation logic for the ContextProcessor node, using Zod schemas from types package
 */

import type { Node } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";
import { ContextProcessorNodeConfigSchema } from "@wf-agent/types";

/**
 * Verify the configuration of the ContextProcessor node
 * @param node: Node definition
 * @returns: Verification result
 */
export function validateContextProcessorNode(
  node: Node,
): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "CONTEXT_PROCESSOR");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, ContextProcessorNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
