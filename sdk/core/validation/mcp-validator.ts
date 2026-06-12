/**
 * MCP Validator
 * Runtime validation of MCP server connection configuration.
 * Schemas are defined in @wf-agent/types and imported here for validation.
 */

import { z } from "zod";
import { McpServerConfigSchema, McpSettingsSchema } from "@wf-agent/types";

/**
 * Validate a single MCP server configuration.
 * @throws {Error} with descriptive message if validation fails
 */
export function validateServerConfig(
  config: unknown,
  serverName?: string,
): z.infer<typeof McpServerConfigSchema> {
  const result = McpServerConfigSchema.safeParse(config);

  if (!result.success) {
    const errorMessages = result.error.issues
      .map(err => `${err.path.join(".")}: ${err.message}`)
      .join("; ");
    throw new Error(
      serverName
        ? `Invalid configuration for server "${serverName}": ${errorMessages}`
        : `Invalid server configuration: ${errorMessages}`,
      { cause: result.error },
    );
  }

  return result.data;
}

/**
 * Validate an entire MCP settings object.
 * @throws {Error} with descriptive message if validation fails
 */
export function validateMcpSettings(settings: unknown): z.infer<typeof McpSettingsSchema> {
  const result = McpSettingsSchema.safeParse(settings);

  if (!result.success) {
    const errorMessages = result.error.issues
      .map(err => `${err.path.join(".")}: ${err.message}`)
      .join("\n");
    throw new Error(`Invalid MCP settings: ${errorMessages}`);
  }

  return result.data;
}
