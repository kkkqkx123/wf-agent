/**
 * Route node validation function
 * Provides static validation logic for Route nodes, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { RouteNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { dslParseWithErrors, scriptCompiler } from "../../../services/evaluation/index.js";
import { validateNodeType, validateNodeConfig } from "../../../shared/validation/utils.js";

/**
 * Validate a single route condition based on its type.
 * Expression conditions are validated via DSL parsing.
 * Script conditions are validated via compilation attempt.
 * Predicate and schema conditions are structurally validated by Zod schema.
 */
function validateRouteCondition(
  condition: Record<string, unknown>,
  nodeId: string,
  routeIndex: number,
): ConfigurationValidationError | null {
  const type = condition["type"] as string;

  if (type === "expression") {
    const result = dslParseWithErrors(condition["expression"] as string);
    if (result.errors.length > 0) {
      return new ConfigurationValidationError(
        `Invalid route condition expression: ${result.errors.map(e => e.message).join("; ")}`,
        {
          configType: "node",
          configPath: `node.${nodeId}.config.routes[${routeIndex}].condition.expression`,
        },
      );
    }
    return null;
  }

  if (type === "script") {
    try {
      scriptCompiler.compile(condition["script"] as string);
    } catch (error) {
      return new ConfigurationValidationError(
        `Invalid route condition script: ${error instanceof Error ? error.message : String(error)}`,
        {
          configType: "node",
          configPath: `node.${nodeId}.config.routes[${routeIndex}].condition.script`,
        },
      );
    }
    return null;
  }

  // predicate and schema conditions are validated by Zod schema structurally
  return null;
}

/**
 * Verify Route node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateRouteNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "ROUTE");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, RouteNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  const config = configResult.value;
  const conditionErrors: ConfigurationValidationError[] = [];

  for (let i = 0; i < config.routes.length; i++) {
    const route = config.routes[i];
    if (!route) continue;
    const error = validateRouteCondition(route.condition as Record<string, unknown>, node.id, i);
    if (error) {
      conditionErrors.push(error);
    }
  }

  if (conditionErrors.length > 0) {
    return err(conditionErrors);
  }

  return ok(node);
}
