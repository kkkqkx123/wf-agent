/**
 * Hook validation function
 * Provides static validation logic for Hook configuration, using zod for validation.
 */

import { z } from "zod";
import type { NodeHook } from "@wf-agent/types";
import { HookType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { validateConfig } from "./utils.js";
import { all } from "@wf-agent/common-utils";

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
  return validateConfig(hook, hookSchema, `node.${nodeId}.hooks`, "node");
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
      return ok(hook);
    }
    return validateHook(hook, nodeId);
  });

  return all(results);
}
