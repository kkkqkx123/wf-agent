/**
 * CLI Configuration Loader (Refactored)
 * Simplified configuration loading without cosmiconfig.
 * Uses SDK's parsing capabilities for TOML and JSON.
 */

import { loadConfigContent, parseJson } from "@wf-agent/sdk";
import { TomlParserManager } from "@wf-agent/sdk";
import type { CLIConfig } from "./types.js";
import { CLIConfigSchema } from "./schema.js";
import { DEFAULT_CONFIG } from "./defaults.js";
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
      // Use TomlParserManager directly for CLI config (not workflow config)
      const toml = TomlParserManager.getInstance();
      return toml.parse(content);
    default:
      throw new Error(`Unsupported config format: ${format}`);
  }
}

/**
 * Load CLI configuration from explicit path or default location
 * @param configPath Explicit config file path (optional)
 * @returns Validated configuration object
 */
export async function loadConfig(configPath?: string): Promise<CLIConfig> {
  // If no path specified, use default location
  const targetPath = configPath || "./.modular-agent.toml";

  try {
    // Use SDK's loadConfigContent
    const { content, format } = await loadConfigContent(targetPath);

    // Parse the content using SDK parsers
    const rawConfig = parseConfigContent(content, format);

    // Validate with Zod
    const validatedConfig = CLIConfigSchema.parse(rawConfig);

    // Merge with defaults
    return { ...DEFAULT_CONFIG, ...validatedConfig };
  } catch (error) {
    // If explicit path was specified and failed, throw error
    if (configPath) {
      throw new Error(
        `Failed to load config from ${configPath}: ${
          error instanceof Error ? error.message : String(error)
        }`,
        { cause: error },
      );
    }

    // If default path failed, use defaults with warning
    output.warnLog("Config file not found or invalid, using default configuration:", {
      error: String(error),
    });
    return CLIConfigSchema.parse(DEFAULT_CONFIG);
  }
}

/**
 * Load configuration with environment variable overrides
 */
export async function loadConfigWithEnvOverride(configPath?: string): Promise<CLIConfig> {
  const config = await loadConfig(configPath);

  // Apply environment variable overrides
  if (process.env["CLI_VERBOSE"] === "true") {
    config.verbose = true;
  }
  if (process.env["CLI_DEBUG"] === "true") {
    config.debug = true;
  }
  if (process.env["CLI_LOG_LEVEL"]) {
    config.logLevel = process.env["CLI_LOG_LEVEL"] as CLIConfig["logLevel"];
  }
  if (process.env["LOG_DIR"] && config.output) {
    config.output = { ...config.output, dir: process.env["LOG_DIR"] };
  }
  // Apply STORAGE_DIR override for test isolation
  if (process.env["STORAGE_DIR"] && config.storage?.json) {
    config.storage.json = { ...config.storage.json, baseDir: process.env["STORAGE_DIR"] };
  }

  return config;
}
