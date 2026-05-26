/**
 * Interactive Script Node Validation Function
 * Provides static validation logic for Interactive Script nodes, using Zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { InteractiveScriptNodeConfigSchema } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify Interactive Script node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateInteractiveScriptNode(node: StaticNode): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "INTERACTIVE_SCRIPT");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, InteractiveScriptNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}