/**
 * LoopEnd Node Validation Function
 * Provides static validation logic for the LoopEnd node, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { LoopEndNodeConfigSchema, ConfigurationValidationError, RuntimeValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateExpression } from "../../evaluation/index.js";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LoopEnd node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateLoopEndNode(node: StaticNode): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "LOOP_END");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, LoopEndNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  const config = configResult.value;
  if (config.breakCondition?.expression) {
    try {
      validateExpression(config.breakCondition.expression);
    } catch (error) {
      if (error instanceof RuntimeValidationError) {
        const validationError = new ConfigurationValidationError(error.message, {
          configType: "node",
          configPath: `node.${node.id}.config.breakCondition.expression`,
        });
        return err([validationError]);
      }
      throw error;
    }
  }

  return ok(node);
}
