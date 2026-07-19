/**
 * CLI Configuration Loader
 * Simplified configuration loading without cosmiconfig.
 * Uses SDK's parsing capabilities for TOML and JSON.
 * Uses centralized environment variable mapping from SDK.
 *
 * Config file loading utilities inherited from @wf-agent/runtime/config.
 */

import { createAppConfigLoader } from "@wf-agent/runtime/config";
import type { EnvMappingEntry } from "@wf-agent/sdk/api";
import type { LogLevel, OutputFormat } from "@wf-agent/types";
import { CLIConfigSchema } from "./schema.js";
import { DEFAULT_CONFIG } from "./defaults.js";
import { getOutput } from "../../utils/output.js";

const output = getOutput();

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

const { loadConfig, loadConfigWithEnvOverride } = createAppConfigLoader({
  defaultConfigFileName: "./.modular-agent.toml",
  schema: CLIConfigSchema,
  defaults: DEFAULT_CONFIG,
  envMapping: CLI_ENV_MAPPING,
  warn: (msg: string) => output.warnLog(msg),
});

export { loadConfig, loadConfigWithEnvOverride };

/**
 * Get the CLI environment mapping definition.
 * Useful for documentation and validation.
 */
export function getCLIEnvMapping() {
  return CLI_ENV_MAPPING;
}