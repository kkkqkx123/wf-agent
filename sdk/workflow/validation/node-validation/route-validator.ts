/**
 * Route node validation function
 * Provides static validation logic for Route nodes, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { RouteNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify Route node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateRouteNode(node: StaticNode): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "ROUTE");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, RouteNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
