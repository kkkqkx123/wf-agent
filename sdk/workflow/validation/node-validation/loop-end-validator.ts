/**
 * LoopEnd Node Validation Function
 * Provides static validation logic for the LoopEnd node, using zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { LoopEndNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { dslParseWithErrors } from "../../evaluation/index.js";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify LoopEnd node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateLoopEndNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
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
    const result = dslParseWithErrors(config.breakCondition.expression);
    if (result.errors.length > 0) {
      const validationError = new ConfigurationValidationError(
        `Invalid break condition expression: ${result.errors.map(e => e.message).join("; ")}`,
        {
          configType: "node",
          configPath: `node.${node.id}.config.breakCondition.expression`,
        },
      );
      return err([validationError]);
    }
  }

  return ok(node);
}
