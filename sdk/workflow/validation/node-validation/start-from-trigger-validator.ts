/**
 * StartFromTrigger node validation function
 * Provides static validation logic for the StartFromTrigger node, using zod for validation.
 */

import { z } from "zod";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * START_FROM_TRIGGER node configuration schema
 * Empty configuration, used only for identification
 */
const startFromTriggerNodeConfigSchema = z.strictObject({});

/**
 * Verify the START_FROM_TRIGGER node
 * @param node: StaticNode definition
 * @returns: Verification result
 */
export function validateStartFromTriggerNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "START_FROM_TRIGGER");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(
    node.config || {},
    startFromTriggerNodeConfigSchema,
    node.id,
  );
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
