/**
 * MCP Configuration Validation Schema
 * Uses Zod for runtime validation of MCP server configurations
 */

import { z } from "zod";
import type { McpServerConfig } from "../types.js";

// Error messages for better user feedback
const TYPE_ERROR_MSG = "Server type must be 'stdio', 'sse', or 'streamable-http'";
const STDIO_FIELDS_ERROR_MSG =
  "For 'stdio' type servers, you must provide a 'command' field and can optionally include 'args' and 'env'";
const SSE_FIELDS_ERROR_MSG =
  "For 'sse' type servers, you must provide a 'url' field and can optionally include 'headers'";
const STREAMABLE_HTTP_FIELDS_ERROR_MSG =
  "For 'streamable-http' type servers, you must provide a 'url' field and can optionally include 'headers'";
const MIXED_FIELDS_ERROR_MSG =
  "Cannot mix 'stdio' and ('sse' or 'streamable-http') fields. For 'stdio' use 'command', 'args', and 'env'. For 'sse'/'streamable-http' use 'url' and 'headers'";
const MISSING_FIELDS_ERROR_MSG =
  "Server configuration must include either 'command' (for stdio) or 'url' (for sse/streamable-http) and a corresponding 'type' if 'url' is used.";

/**
 * Base configuration schema for common settings
 */
const BaseConfigSchema = z.object({
  disabled: z.boolean().optional(),
  timeout: z.number().min(1).max(3600).optional().default(60),
  alwaysAllow: z.array(z.string()).default([]),
  watchPaths: z.array(z.string()).optional(),
  disabledTools: z.array(z.string()).default([]),
});

/**
 * Stdio configuration schema
 */
const StdioConfigSchema = BaseConfigSchema.extend({
  type: z.enum(["stdio"]).optional(),
  command: z.string().min(1, "Command cannot be empty"),
  args: z.array(z.string()).optional(),
  cwd: z.string().optional(),
  env: z.record(z.string(), z.string()).optional(),
  url: z.undefined().optional(),
  headers: z.undefined().optional(),
}).transform((data) => ({
  ...data,
  type: "stdio" as const,
}));

/**
 * SSE configuration schema
 */
const SseConfigSchema = BaseConfigSchema.extend({
  type: z.enum(["sse"]).optional(),
  url: z.string().url("URL must be a valid URL format"),
  headers: z.record(z.string(), z.string()).optional(),
  command: z.undefined().optional(),
  args: z.undefined().optional(),
  env: z.undefined().optional(),
}).transform((data) => ({
  ...data,
  type: "sse" as const,
}));

/**
 * Streamable HTTP configuration schema
 */
const StreamableHttpConfigSchema = BaseConfigSchema.extend({
  type: z.enum(["streamable-http"]).optional(),
  url: z.string().url("URL must be a valid URL format"),
  headers: z.record(z.string(), z.string()).optional(),
  command: z.undefined().optional(),
  args: z.undefined().optional(),
  env: z.undefined().optional(),
}).transform((data) => ({
  ...data,
  type: "streamable-http" as const,
}));

/**
 * Server configuration schema (union of all types)
 */
export const ServerConfigSchema = z.union([
  StdioConfigSchema,
  SseConfigSchema,
  StreamableHttpConfigSchema,
]);

/**
 * Settings schema for the entire MCP settings file
 */
export const McpSettingsSchema = z.object({
  mcpServers: z.record(z.string(), ServerConfigSchema),
});

/**
 * Validate server configuration with detailed error messages
 *
 * @param config - Raw server configuration
 * @param serverName - Optional server name for error messages
 * @returns Validated configuration
 * @throws Error if configuration is invalid
 */
export function validateServerConfig(
  config: unknown,
  serverName?: string
): McpServerConfig {
  // Detect configuration issues before validation
  const configObj = config as Record<string, unknown>;
  const hasStdioFields = configObj["command"] !== undefined;
  const hasUrlFields = configObj["url"] !== undefined;

  // Check for mixed fields (stdio vs url-based)
  if (hasStdioFields && hasUrlFields) {
    throw new Error(MIXED_FIELDS_ERROR_MSG);
  }

  // Infer type for stdio if not provided
  if (!configObj["type"] && hasStdioFields) {
    configObj["type"] = "stdio";
  }

  // For url-based configs, type must be provided by the user
  if (hasUrlFields && !configObj["type"]) {
    throw new Error(
      "Configuration with 'url' must explicitly specify 'type' as 'sse' or 'streamable-http'."
    );
  }

  // Validate type if provided
  const configType = configObj["type"];
  if (
    configType &&
    !["stdio", "sse", "streamable-http"].includes(configType as string)
  ) {
    throw new Error(TYPE_ERROR_MSG);
  }

  // Check for type/field mismatch
  if (configObj["type"] === "stdio" && !hasStdioFields) {
    throw new Error(STDIO_FIELDS_ERROR_MSG);
  }
  if (configObj["type"] === "sse" && !hasUrlFields) {
    throw new Error(SSE_FIELDS_ERROR_MSG);
  }
  if (configObj["type"] === "streamable-http" && !hasUrlFields) {
    throw new Error(STREAMABLE_HTTP_FIELDS_ERROR_MSG);
  }

  // If neither command nor url is present (type alone is not enough)
  if (!hasStdioFields && !hasUrlFields) {
    throw new Error(MISSING_FIELDS_ERROR_MSG);
  }

  // Validate the config against the schema
  try {
    return ServerConfigSchema.parse(config) as McpServerConfig;
  } catch (validationError) {
    if (validationError instanceof z.ZodError) {
      // Extract and format validation errors
      const errorMessages = validationError.issues
        .map((err) => `${err.path.join(".")}: ${err.message}`)
        .join("; ");
      throw new Error(
        serverName
          ? `Invalid configuration for server "${serverName}": ${errorMessages}`
          : `Invalid server configuration: ${errorMessages}`
      );
    }
    throw validationError;
  }
}

/**
 * Validate MCP settings file
 *
 * @param settings - Raw settings object
 * @returns Validated settings
 * @throws Error if settings are invalid
 */
export function validateMcpSettings(settings: unknown): z.infer<typeof McpSettingsSchema> {
  const result = McpSettingsSchema.safeParse(settings);

  if (!result.success) {
    const errorMessages = result.error.issues
      .map((err) => `${err.path.join(".")}: ${err.message}`)
      .join("\n");
    throw new Error(`Invalid MCP settings: ${errorMessages}`);
  }

  return result.data;
}

/**
 * Check if a configuration is a stdio config
 */
export function isStdioConfig(
  config: McpServerConfig
): config is McpServerConfig & { type: "stdio"; command: string } {
  return config.type === "stdio";
}

/**
 * Check if a configuration is an SSE config
 */
export function isSseConfig(
  config: McpServerConfig
): config is McpServerConfig & { type: "sse"; url: string } {
  return config.type === "sse";
}

/**
 * Check if a configuration is a streamable HTTP config
 */
export function isStreamableHttpConfig(
  config: McpServerConfig
): config is McpServerConfig & { type: "streamable-http"; url: string } {
  return config.type === "streamable-http";
}
