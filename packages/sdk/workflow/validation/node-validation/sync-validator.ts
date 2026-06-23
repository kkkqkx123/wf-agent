/**
 * Sync Node Validation Function
 * Provides static validation logic for Sync nodes, using Zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { SyncNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../shared/validation/utils.js";

/**
 * Verify Sync node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateSyncNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "SYNC");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, SyncNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
