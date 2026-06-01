/**
 * Hook validation function
 * Provides static validation logic for Hook configuration, using zod for validation.
 */

import { z } from "zod";
import type { NodeHook } from "@wf-agent/types";
import { HookType } from "@wf-agent/types";
import { ConfigurationValidationError, ExpressionSecurityError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateExpression } from "../../workflow/evaluation/index.js";
import type { Result } from "@wf-agent/types";
import { validateConfig } from "./utils.js";
import { allWithErrors } from "@wf-agent/common-utils";

/**
 * Hook configuration schema
 */
const hookSchema = z.object({
  hookType: z.custom<HookType>((val): val is HookType =>
    ["BEFORE_EXECUTE", "AFTER_EXECUTE"].includes(val as HookType),
  ),
  enabled: z.boolean().optional(),
  weight: z.number().optional(),
  condition: z
    .object({
      expression: z.string().min(1, "Condition expression is required"),
      metadata: z.any().optional(),
    })
    .optional(),
  eventName: z.string().min(1, "Event name is required"),
  eventPayload: z.record(z.string(), z.any()).optional(),
});

/**
 * Verify Hook Configuration
 * @param hook Hook configuration
 * @param nodeId Node ID (used for error paths)
 * @throws ValidationError Throws a ValidationError when the configuration is invalid
 */
export function validateHook(
  hook: NodeHook,
  nodeId: string,
): Result<NodeHook, ConfigurationValidationError[]> {
  const result = validateConfig(hook, hookSchema, `node.${nodeId}.hooks`, "node");
  if (result.isErr()) {
    return result;
  }

  const validatedHook = result.value;
  if (validatedHook.condition?.expression) {
    try {
      validateExpression(validatedHook.condition.expression);
    } catch (error) {
      if (error instanceof ExpressionSecurityError) {
        return err([
          new ConfigurationValidationError(error.message, {
            configType: "node",
            configPath: `node.${nodeId}.hooks.condition.expression`,
          }),
        ]);
      }
      throw error;
    }
  }

  return ok(validatedHook);
}

/**
 * Verify the Hook array
 * @param hooks The Hook array
 * @param nodeId The node ID (used for error paths)
 * @throws ValidationError Throws a ValidationError when the configuration is invalid
 */
export function validateHooks(
  hooks: NodeHook[],
  nodeId: string,
): Result<NodeHook[], ConfigurationValidationError[]> {
  if (!hooks || !Array.isArray(hooks)) {
    return err([
      new ConfigurationValidationError("Hooks must be an array", {
        configType: "node",
        configPath: `node.${nodeId}.hooks`,
      }),
    ]);
  }

  const results = hooks.map(hook => {
    if (!hook) {
      return err([
        new ConfigurationValidationError("Hook must not be null or undefined", {
          configType: "node",
          configPath: `node.${nodeId}.hooks`,
        }),
      ]);
    }
    return validateHook(hook, nodeId);
  });

  const combined = allWithErrors(results);
  if (combined.isErr()) {
    return err(combined.error.flat());
  }
  return combined;
}
