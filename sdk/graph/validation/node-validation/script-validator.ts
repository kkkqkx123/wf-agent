/**
 * Code Node Validation Function
 * Provides static validation logic for Code nodes, using Zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { ScriptNodeConfigSchema } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify Code node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateScriptNode(node: Node): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "SCRIPT");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, ScriptNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
