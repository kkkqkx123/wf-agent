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

import * as fs from "fs/promises";
import * as path from "path";
import { getConfigFormatFromPath, type ConfigFormat as SDKConfigFormat } from "@wf-agent/sdk/api";
import { parseJson, parseToml } from "@wf-agent/sdk/api";
import { applyEnvOverrides, type EnvMapping } from "@wf-agent/sdk/api";
import { ExecutionModeEnvVars } from "../mode/index.js";
import type { z } from "zod";

// ============================================
// File I/O helpers
// ============================================

/**
 * Read configuration file content from disk.
 * @param filePath - Configuration file path.
 * @returns File content as string.
 * @throws Error if file cannot be read.
 */
export async function readConfigFile(filePath: string): Promise<string> {
  try {
    return await fs.readFile(filePath, "utf-8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      throw new Error(`Configuration file not found: ${filePath}`, { cause: error });
    }
    throw error;
  }
}

/**
 * Load configuration file content and detect format.
 * @param filePath - Configuration file path.
 * @returns Object containing file content and detected format.
 * @throws Error if file cannot be read or format is not recognised.
 */
export async function loadConfigFile(
  filePath: string,
): Promise<{ content: string; format: SDKConfigFormat }> {
  const content = await readConfigFile(filePath);
  const format = getConfigFormatFromPath(filePath);
  return { content, format };
}

/**
 * Safely load configuration file content and detect format.
 * Returns null if the file does not exist; throws on other errors.
 * @param filePath - Configuration file path.
 * @returns Object containing file content and detected format, or null if file not found.
 */
export async function tryLoadConfigFile(
  filePath: string,
): Promise<{ content: string; format: SDKConfigFormat } | null> {
  try {
    return await loadConfigFile(filePath);
  } catch (error) {
    if ((error as Error).message?.startsWith("Configuration file not found")) {
      return null;
    }
    throw error;
  }
}

// ============================================
// Config format helpers
// ============================================

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

// ============================================
// App Config Loader Factory
// ============================================

/**
 * Options for creating an app-specific config loader.
 */
export interface AppConfigLoaderOptions<T extends Record<string, unknown>> {
  /** Default config file name (e.g., ".modular-agent.toml") */
  defaultConfigFileName: string;
  /** Zod schema for validation */
  schema: z.ZodType<T>;
  /** Default config values */
  defaults: T;
  /** Environment variable mapping */
  envMapping: EnvMapping<T>;
  /** Warn logger (defaults to console.warn) */
  warn?: (msg: string) => void;
}

/**
 * App-specific config loader with loadConfig and loadConfigWithEnvOverride.
 */
export interface AppConfigLoader<T extends Record<string, unknown>> {
  /** Load config from file, falling back to defaults if no explicit path is given. */
  loadConfig: (configPath?: string) => Promise<T>;
  /** Load config then apply environment variable overrides. */
  loadConfigWithEnvOverride: (configPath?: string) => Promise<T>;
}

/**
 * Create an app-specific config loader.
 *
 * Eliminates the duplicated loadConfig / loadConfigWithEnvOverride pattern
 * between cli-app and server by providing a single factory.
 *
 * @example
 * ```ts
 * const loader = createAppConfigLoader({
 *   defaultConfigFileName: ".modular-agent.toml",
 *   schema: CLIConfigSchema,
 *   defaults: DEFAULT_CONFIG,
 *   envMapping: CLI_ENV_MAPPING,
 * });
 * const config = await loader.loadConfigWithEnvOverride();
 * ```
 */
export function createAppConfigLoader<T extends Record<string, unknown>>(
  options: AppConfigLoaderOptions<T>,
): AppConfigLoader<T> {
  const {
    defaultConfigFileName,
    schema,
    defaults,
    envMapping,
    warn = console.warn,
  } = options;

  const loadConfig = async (configPath?: string): Promise<T> => {
    const targetPath = configPath || defaultConfigFileName;

    try {
      const { content, format } = await loadConfigFromFile(targetPath);
      const rawConfig = parseConfigContent(content, format);
      const validatedConfig = schema.parse(rawConfig);
      return { ...defaults, ...validatedConfig } as T;
    } catch (error) {
      if (configPath) {
        throw new Error(
          `Failed to load config from ${configPath}: ${
            error instanceof Error ? error.message : String(error)
          }`,
          { cause: error },
        );
      }

      warn(`Config file not found or invalid, using default configuration: ${String(error)}`);
      return schema.parse(defaults) as T;
    }
  };

  const loadConfigWithEnvOverride = async (configPath?: string): Promise<T> => {
    const config = await loadConfig(configPath);
    const result = applyEnvOverrides(config, envMapping);

    // Handle LOG_DIR environment variable
    const logDir = process.env["LOG_DIR"];
    if (logDir && (result as Record<string, unknown>)["output"]) {
      const output = (result as Record<string, unknown>)["output"] as Record<string, unknown>;
      output["dir"] = logDir;
    }

    // Handle STORAGE_DIR environment variable
    const storageDir = process.env["STORAGE_DIR"];
    if (storageDir) {
      const storage = (result as Record<string, unknown>)["storage"] as
        | Record<string, unknown>
        | undefined;
      if (storage?.["sqlite"]) {
        const sqlite = storage["sqlite"] as Record<string, unknown>;
        const sqlitePath = sqlite["dbPath"] as string;
        // Use path.basename to extract the filename portion, then
        // join with storageDir — this works on both Windows and POSIX.
        const dbFileName = path.basename(sqlitePath);
        const newDbPath = path.join(storageDir, dbFileName);
        sqlite["dbPath"] = newDbPath;
      }
    }

    // Handle ExecutionMode output format interaction
    const envFormat = process.env[ExecutionModeEnvVars.OUTPUT_FORMAT];
    if (envFormat === "json" || envFormat === "silent") {
      (result as Record<string, unknown>)["outputFormat"] = envFormat;
    } else if ((result as Record<string, unknown>)["outputFormat"] === "json" && !envFormat) {
      process.env[ExecutionModeEnvVars.OUTPUT_FORMAT] = "json";
    }

    return result;
  };

  return { loadConfig, loadConfigWithEnvOverride };
}