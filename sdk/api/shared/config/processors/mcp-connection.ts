/**
 * MCP Connection Processor
 *
 * Pure functions for MCP connection configuration validation, parsing,
 * merging, and default creation. No file I/O — that belongs in loaders/.
 */

import { z } from "zod";
import type { McpServerConfig, McpSettings } from "@wf-agent/types";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "MCPConnectionProcessor" });

// ---------------------------------------------------------------------------
// Error messages
// ---------------------------------------------------------------------------
const TYPE_ERROR_MSG = "Server type must be 'stdio', 'sse', or 'streamable-http'";
const STDIO_FIELDS_ERROR_MSG =
  "For 'stdio' type servers, you must provide a 'command' field and can optionally include 'args' and 'env'";
const SSE_FIELDS_ERROR_MSG =
  "For 'sse' type servers, you must provide a 'url' field and can optionally include 'headers'";
const STREAMABLE_HTTP_FIELDS_ERROR_MSG =
  "For 'streamable-http' type servers, you must provide a 'url' field and can optionally include 'headers'";
const MISSING_FIELDS_ERROR_MSG =
  "Server configuration must include either 'command' (for stdio) or 'url' (for sse/streamable-http) and a corresponding 'type' if 'url' is used.";

// ---------------------------------------------------------------------------
// Zod Schemas
// ---------------------------------------------------------------------------

const BaseConfigSchema = z.object({
  disabled: z.boolean().optional(),
  timeout: z.number().min(1).max(3600).optional().default(60),
  alwaysAllow: z.array(z.string()).default([]),
  watchPaths: z.array(z.string()).optional(),
  disabledTools: z.array(z.string()).default([]),
});

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
 * Server configuration schema (union of all transport types)
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

// ---------------------------------------------------------------------------
// Validation functions
// ---------------------------------------------------------------------------

/**
 * Validate a single server configuration with detailed error messages.
 */
export function validateServerConfig(
  config: unknown,
  serverName?: string,
): McpServerConfig {
  const configObj = config as Record<string, unknown>;
  const hasStdioFields = configObj["command"] !== undefined;
  const hasUrlFields = configObj["url"] !== undefined;

  if (hasStdioFields && hasUrlFields) {
    throw new Error(
      "Cannot mix 'stdio' and ('sse' or 'streamable-http') fields. For 'stdio' use 'command', 'args', and 'env'. For 'sse'/'streamable-http' use 'url' and 'headers'",
    );
  }

  if (!configObj["type"] && hasStdioFields) {
    configObj["type"] = "stdio";
  }

  if (hasUrlFields && !configObj["type"]) {
    throw new Error(
      "Configuration with 'url' must explicitly specify 'type' as 'sse' or 'streamable-http'.",
    );
  }

  const configType = configObj["type"];
  if (configType && !["stdio", "sse", "streamable-http"].includes(configType as string)) {
    throw new Error(TYPE_ERROR_MSG);
  }

  if (configObj["type"] === "stdio" && !hasStdioFields) {
    throw new Error(STDIO_FIELDS_ERROR_MSG);
  }
  if (configObj["type"] === "sse" && !hasUrlFields) {
    throw new Error(SSE_FIELDS_ERROR_MSG);
  }
  if (configObj["type"] === "streamable-http" && !hasUrlFields) {
    throw new Error(STREAMABLE_HTTP_FIELDS_ERROR_MSG);
  }

  if (!hasStdioFields && !hasUrlFields) {
    throw new Error(MISSING_FIELDS_ERROR_MSG);
  }

  try {
    return ServerConfigSchema.parse(config) as McpServerConfig;
  } catch (validationError) {
    if (validationError instanceof z.ZodError) {
      const errorMessages = validationError.issues
        .map((err) => `${err.path.join(".")}: ${err.message}`)
        .join("; ");
      throw new Error(
        serverName
          ? `Invalid configuration for server "${serverName}": ${errorMessages}`
          : `Invalid server configuration: ${errorMessages}`,
        { cause: validationError },
      );
    }
    throw validationError;
  }
}

/**
 * Validate an entire MCP settings object.
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

// ---------------------------------------------------------------------------
// Type guards
// ---------------------------------------------------------------------------

export function isStdioConfig(
  config: McpServerConfig,
): config is McpServerConfig & { type: "stdio"; command: string } {
  return config.type === "stdio";
}

export function isSseConfig(
  config: McpServerConfig,
): config is McpServerConfig & { type: "sse"; url: string } {
  return config.type === "sse";
}

export function isStreamableHttpConfig(
  config: McpServerConfig,
): config is McpServerConfig & { type: "streamable-http"; url: string } {
  return config.type === "streamable-http";
}

// ---------------------------------------------------------------------------
// Processing / merging
// ---------------------------------------------------------------------------

/**
 * Load server configurations from parsed settings, skipping invalid entries.
 */
export function loadServerConfigs(
  settings: McpSettings,
): Map<string, McpServerConfig> {
  const configs = new Map<string, McpServerConfig>();

  for (const [name, config] of Object.entries(settings.mcpServers || {})) {
    try {
      const validatedConfig = validateServerConfig(config, name);
      configs.set(name, validatedConfig);
    } catch (error) {
      logger.error(`Failed to validate config for server "${name}"`, { error: getErrorOrNew(error) });
    }
  }

  return configs;
}

/**
 * Create default empty MCP settings.
 */
export function createDefaultMcpSettings(): McpSettings {
  return { mcpServers: {} };
}

/**
 * Merge server configurations from multiple sources.
 * Project-level servers take priority over global servers.
 */
export function mergeServerConfigs(
  globalConfigs: Map<string, McpServerConfig>,
  projectConfigs: Map<string, McpServerConfig>,
): Map<string, McpServerConfig> {
  const merged = new Map<string, McpServerConfig>(globalConfigs);
  for (const [name, config] of projectConfigs) {
    merged.set(name, config);
  }
  return merged;
}