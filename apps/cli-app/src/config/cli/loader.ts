/**
 * CLI Configuration Loader (Refactored)
 * Simplified configuration loading without cosmiconfig.
 * Uses SDK's parsing capabilities for TOML and JSON.
 * Uses centralized environment variable mapping from SDK.
 */

import { loadConfigFile } from "@wf-agent/config-processor";
import {
  parseJson,
  parseToml,
  applyEnvOverrides,
  EnvMappingEntry,
} from "@wf-agent/sdk/api";
import type { CLIConfig } from "./types.js";
import type { LogLevel, OutputFormat } from "@wf-agent/types";
import { CLIConfigSchema } from "./schema.js";
import { DEFAULT_CONFIG } from "./defaults.js";
import { ExecutionModeEnvVars } from "../../types/execution-mode.js";
import { getOutput } from "../../utils/output.js";

const output = getOutput();

/**
 * Parse configuration content based on format using SDK parsers
 */
function parseConfigContent(content: string, format: "json" | "toml"): unknown {
  switch (format) {
    case "json":
      return parseJson(content);
    case "toml":
      return parseToml(content);
    default:
      throw new Error(`Unsupported config format: ${format}`);
  }
}

/**
 * CLI environment variable mapping definition.
 * Uses SDK's centralized EnvMappingEntry type for declarative mapping.
 */
const CLI_ENV_MAPPING: Record<string, EnvMappingEntry> = {
  verbose: { env: "CLI_VERBOSE", parser: (v: string) => v === "true" || v === "1" },
  debug: { env: "CLI_DEBUG", parser: (v: string) => v === "true" || v === "1" },
  logLevel: { env: "CLI_LOG_LEVEL", parser: (v: string) => v as LogLevel },
  outputFormat: { env: "CLI_OUTPUT_FORMAT", parser: (v: string) => v as OutputFormat },
  defaultTimeout: { env: "CLI_DEFAULT_TIMEOUT", parser: (v: string) => parseInt(v, 10) },
  maxConcurrentExecutions: { env: "CLI_MAX_CONCURRENT", parser: (v: string) => parseInt(v, 10) },
};

/**
 * Load CLI configuration from explicit path or default location
 * @param configPath Explicit config file path (optional)
 * @returns Validated configuration object
 */
export async function loadConfig(configPath?: string): Promise<CLIConfig> {
  const targetPath = configPath || "./.modular-agent.toml";

  try {
    const { content, format } = await loadConfigFile(targetPath);
    const rawConfig = parseConfigContent(content, format);
    const validatedConfig = CLIConfigSchema.parse(rawConfig);
    return { ...DEFAULT_CONFIG, ...validatedConfig };
  } catch (error) {
    if (configPath) {
      throw new Error(
        `Failed to load config from ${configPath}: ${
          error instanceof Error ? error.message : String(error)
        }`,
        { cause: error },
      );
    }

    output.warnLog("Config file not found or invalid, using default configuration:", {
      error: String(error),
    });
    return CLIConfigSchema.parse(DEFAULT_CONFIG);
  }
}

/**
 * Load configuration with environment variable overrides.
 * Uses SDK's centralized env mapping system.
 */
export async function loadConfigWithEnvOverride(configPath?: string): Promise<CLIConfig> {
  const config = await loadConfig(configPath);

  const result = applyEnvOverrides(config as unknown as Record<string, unknown>, CLI_ENV_MAPPING) as unknown as CLIConfig;

  if (process.env["LOG_DIR"] && result.output) {
    result.output = { ...result.output, dir: process.env["LOG_DIR"] };
  }
  if (process.env["STORAGE_DIR"]) {
    if (result.storage?.json) {
      result.storage.json = { ...result.storage.json, baseDir: process.env["STORAGE_DIR"] };
    }
    if (result.storage?.sqlite) {
      const sqlitePath = result.storage.sqlite.dbPath;
      const dirName = sqlitePath.substring(0, sqlitePath.lastIndexOf("/") + 1) || "./";
      const newDbPath = process.env["STORAGE_DIR"] + sqlitePath.substring(dirName.length);
      result.storage.sqlite = { ...result.storage.sqlite, dbPath: newDbPath };
    }
  }

  const envFormat = process.env[ExecutionModeEnvVars.OUTPUT_FORMAT];
  if (envFormat === "json" || envFormat === "silent") {
    result.outputFormat = envFormat as OutputFormat;
  } else if (result.outputFormat === "json" && !envFormat) {
    process.env[ExecutionModeEnvVars.OUTPUT_FORMAT] = "json";
  }

  return result;
}

/**
 * Get the CLI environment mapping definition.
 * Useful for documentation and validation.
 */
export function getCLIEnvMapping() {
  return CLI_ENV_MAPPING;
}
