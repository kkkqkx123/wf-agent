/**
 * Runtime Config Loader
 * Shared configuration file loading and parsing utilities.
 *
 * Provides generic config loading utilities that both cli-app and server
 * can use, eliminating duplicated config loading logic.
 *
 * Uses SDK's built-in parsers (parseJson, parseToml) which handle
 * lazy-loading of @iarna/toml internally.
 *
 * Usage:
 *   import { parseConfigContent, loadConfigFromFile } from "@wf-agent/runtime/config";
 *
 *   const { content, format } = await loadConfigFromFile("./config.toml");
 *   const parsed = parseConfigContent(content, format);
 */

import { loadConfigFile } from "@wf-agent/config-processor";
import { parseJson, parseToml } from "@wf-agent/sdk/api";

/**
 * Supported config file formats
 */
export type ConfigFormat = "json" | "toml";

/**
 * Result of loading a config file
 */
export interface LoadedConfig {
  /** Raw config content string */
  content: string;
  /** Detected format */
  format: ConfigFormat;
}

/**
 * Parse configuration content based on format using SDK parsers.
 *
 * @param content Raw config file content
 * @param format Config format ("json" or "toml")
 * @returns Parsed configuration object
 */
export function parseConfigContent(content: string, format: ConfigFormat): unknown {
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
 * Load a configuration file from disk.
 * Uses @wf-agent/config-processor to handle file discovery and reading.
 *
 * @param configPath Path to the config file
 * @returns Loaded config content and detected format
 */
export async function loadConfigFromFile(configPath: string): Promise<LoadedConfig> {
  const { content, format } = await loadConfigFile(configPath);

  if (format !== "json" && format !== "toml") {
    throw new Error(`Unsupported config format: ${format}`);
  }

  return { content, format: format as ConfigFormat };
}