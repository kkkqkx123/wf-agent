/**
 * ADD_TOOL node validation function
 * Provides static validation logic for the ADD_TOOL node, using zod for validation.
 */

import type { Node } from "@wf-agent/types";
import { AddToolNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Tool Presence Check Interface
 */
interface HasToolChecker {
  hasTool(toolId: string): boolean;
}

/**
 * Verify the configuration of the ADD_TOOL node
 * @param node Node definition
 * @param toolChecker Tool checker (optional, used to verify the existence of the tool, such as ToolRegistry)
 * @returns Verification result
 */
export function validateAddToolNode(
  node: Node,
  toolChecker?: HasToolChecker,
): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "ADD_TOOL");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, AddToolNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  // If a tool checker is provided, verify the existence of the validation tool.
  if (toolChecker) {
    const config = node.config as { toolIds: string[] };
    const invalidToolIds: string[] = [];

    for (const toolId of config.toolIds) {
      if (!toolChecker.hasTool(toolId)) {
        invalidToolIds.push(toolId);
      }
    }

    if (invalidToolIds.length > 0) {
      return err([
        new ConfigurationValidationError(
          `Tool IDs not found in registry: ${invalidToolIds.join(", ")}`,
          {
            configType: "node",
            configPath: `node.${node.id}.config.toolIds`,
            value: invalidToolIds,
          },
        ),
      ]);
    }
  }

  return ok(node);
}
