/**
 * ContinueFromTrigger node validation function
 * Provides static validation logic for the ContinueFromTrigger node, using Zod schemas from types package
 */

import type { Node } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";
import { ContinueFromTriggerNodeConfigSchema } from "@wf-agent/types";

/**
 * Verify the configuration of the ContinueFromTrigger node
 * @param node: Node definition
 * @returns: Verification result
 */
export function validateContinueFromTriggerNode(
  node: Node,
): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "CONTINUE_FROM_TRIGGER");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(
    node.config || {},
    ContinueFromTriggerNodeConfigSchema,
    node.id,
  );
  if (configResult.isErr()) {
    return configResult;
  }

  // Verify the configuration logic.
  const config = node.config as {
    variableCallback?: {
      includeAll?: boolean;
      includeVariables?: string[];
    };
  };

  // If `variableCallback` is configured, `includeAll` and `includeVariables` cannot be set at the same time.
  if (config.variableCallback?.includeAll && config.variableCallback?.includeVariables) {
    return err([
      new ConfigurationValidationError(
        "variableCallback cannot have both includeAll and includeVariables",
        {
          configType: "node",
          configPath: `node.${node.id}.config.variableCallback`,
        },
      ),
    ]);
  }

  return ok(node);
}
