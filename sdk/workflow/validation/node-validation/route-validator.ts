/**
 * Route node validation function
 * Provides static validation logic for Route nodes, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { RouteNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { dslParseWithErrors } from "../../evaluation/index.js";
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

  const config = configResult.value;
  const expressionErrors: ConfigurationValidationError[] = [];

  for (let i = 0; i < config.routes.length; i++) {
    const route = config.routes[i];
    if (!route) continue;
    const result = dslParseWithErrors(route.condition.expression);
    if (result.errors.length > 0) {
      expressionErrors.push(
        new ConfigurationValidationError(
          `Invalid route condition expression: ${result.errors.map(e => e.message).join("; ")}`,
          {
            configType: "node",
            configPath: `node.${node.id}.config.routes[${i}].condition.expression`,
          },
        ),
      );
    }
  }

  if (expressionErrors.length > 0) {
    return err(expressionErrors);
  }

  return ok(node);
}
