/**
 * Verify helper functions
 * Provide generic validation logic to reduce duplicate code
 */

import { z } from "zod";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Verify node configuration
 * @param config Configuration object
 * @param schema Zod schema
 * @param nodeId Node ID
 * @returns Verification result
 */
export function validateNodeConfig<T>(
  config: unknown,
  schema: z.ZodType<T>,
  nodeId: string,
): Result<T, ConfigurationValidationError[]> {
  const result = schema.safeParse(config);
  if (!result.success) {
    const errors = result.error.issues.map(
      issue =>
        new ConfigurationValidationError(issue.message, {
          configType: "node",
          configPath: `node.${nodeId}.config.${issue.path.join(".")}`,
        }),
    );
    return err(errors);
  }
  return ok(result.data);
}

/**
 * Verify node type
 * @param node: Node definition
 * @param expectedType: Expected node type
 * @returns: Verification result
 */
export function validateNodeType(
  node: { type: string; id?: string },
  expectedType: string,
): Result<void, ConfigurationValidationError[]> {
  if (node.type !== expectedType) {
    return err([
      new ConfigurationValidationError(
        `Invalid node type for ${expectedType} validator: ${node.type}`,
        {
          configType: "node",
          configPath: `node.${node.id}`,
        },
      ),
    ]);
  }
  return ok(undefined);
}

/**
 * Verify the configuration object
 * @param config The configuration object
 * @param schema The Zod schema
 * @param configPath The configuration path
 * @param configType The configuration type
 * @returns The verification result
 */
export function validateConfig<T>(
  config: unknown,
  schema: z.ZodType<T>,
  configPath: string,
  configType:
    | "tool"
    | "workflow"
    | "node"
    | "trigger"
    | "edge"
    | "variable"
    | "script"
    | "schema"
    | "llm" = "schema",
): Result<T, ConfigurationValidationError[]> {
  const result = schema.safeParse(config);
  if (!result.success) {
    const errors = result.error.issues.map(
      issue =>
        new ConfigurationValidationError(issue.message, {
          configType,
          configPath: `${configPath}.${issue.path.join(".")}`,
        }),
    );
    return err(errors);
  }
  return ok(result.data);
}

/**
 * Convert the Zod error into an array of validation errors
 * @param error Zod error
 * @param prefix field path prefix
 * @param configType configuration type
 * @returns array of validation errors
 */
export function convertZodError(
  error: z.ZodError,
  prefix?: string,
  configType:
    | "tool"
    | "workflow"
    | "node"
    | "trigger"
    | "edge"
    | "variable"
    | "script"
    | "schema"
    | "llm" = "schema",
): ConfigurationValidationError[] {
  return error.issues.map(issue => {
    const field =
      issue.path.length > 0
        ? prefix
          ? `${prefix}.${issue.path.join(".")}`
          : issue.path.join(".")
        : prefix;
    return new ConfigurationValidationError(issue.message, {
      configType,
      configPath: field,
    });
  });
}
