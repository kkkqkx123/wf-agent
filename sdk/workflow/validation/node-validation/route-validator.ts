/**
 * Route node validation function
 * Provides static validation logic for Route nodes, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { RouteNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateExpression } from "../../evaluation/index.js";
import { RuntimeValidationError } from "@wf-agent/types";
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
    try {
      validateExpression(route.condition.expression);
    } catch (error) {
      if (error instanceof RuntimeValidationError) {
        expressionErrors.push(
          new ConfigurationValidationError(error.message, {
            configType: "node",
            configPath: `node.${node.id}.config.routes[${i}].condition.expression`,
          }),
        );
      } else {
        throw error;
      }
    }
  }

  if (expressionErrors.length > 0) {
    return err(expressionErrors);
  }

  return ok(node);
}
