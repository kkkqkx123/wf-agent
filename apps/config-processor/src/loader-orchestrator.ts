/**
 * Loader Orchestrator
 *
 * Orchestrates the "read file → detect format → parse → merge" pipeline
 * for each config domain.  Every function here follows the same pattern:
 *
 *   1. Try each file path (stop at first success)
 *   2. Read raw content via SDK's `tryLoadConfigFile`
 *   3. Parse via SDK's `parseJson` / `parseToml`
 *   4. Merge with defaults via the corresponding SDK processor
 *   5. Return the fully resolved config
 *
 * Because file I/O is an application-layer concern, these functions live
 * in apps/config-processor, NOT in the SDK.
 */

import { tryLoadConfigFile } from "./config-file-loader.js";
import * as fs from "fs/promises";
import * as path from "path";
import {
  parseJson,
  parseToml,
  parseLLMProfile,
  parseNodeTemplate,
  parseTriggerTemplate,
  parseHookTemplate,
  parseScript,
  parsePromptTemplateConfig,
  validateLLMProfile,
  validateNodeTemplate,
  validateTriggerTemplate,
  validateHookTemplate,
  validateScript,
  validatePromptTemplate,
  mergeMetricsWithDefaults,
  mergeTimeoutWithDefaults,
  mergeFileCheckpointConfig,
  mergeStorageWithDefaults,
  mergeOutputWithDefaults,
  transformPresetsConfig,
  transformReadFileConfig,
  mergeSandboxWithDefaults,
} from "@wf-agent/sdk/api";
import { validateAgentLoopConfig } from "@wf-agent/sdk/agent";
import type {
  PresetsConfig,
  PresetsConfigInput,
  PredefinedToolsPresetConfig,
} from "@wf-agent/sdk/resources";
import type {
  MetricsConfig,
  TimeoutConfig,
  FileCheckpointConfig,
  StorageConfig,
  OutputConfig,
  ValidationError,
  InfrastructurePresetFile,
  SandboxGlobalConfig,
} from "@wf-agent/types";
import type {
  ParsedAgentLoopConfig,
  AgentLoopConfigFile,
  ParsedLLMProfileConfig,
  ParsedNodeTemplateConfig,
  ParsedTriggerTemplateConfig,
  ParsedHookTemplateConfig,
  ParsedScriptConfig,
  ParsedPromptTemplateConfig,
  PromptTemplateConfigFile,
} from "@wf-agent/sdk/api";

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

/**
 * Attempt to load and parse a single config file.
 * Returns the raw parsed object (unknown) or null if the file is missing.
 */
async function tryLoadRawConfig(
  filePath: string,
): Promise<Record<string, unknown> | null> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) return null;

  const { content, format } = loaded;
  const parsed: unknown =
    format === "toml" ? parseToml(content) : parseJson(content);
  return parsed as Record<string, unknown>;
}

// -----------------------------------------------------------------------
// Domain-specific loaders
// -----------------------------------------------------------------------

/**
 * Load metrics config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged MetricsConfig (with defaults applied).
 */
export async function loadMetricsConfig(
  configPaths: string[],
): Promise<MetricsConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeMetricsWithDefaults(raw as Partial<MetricsConfig>);
    }
  }

  // No file found — return pure defaults
  return mergeMetricsWithDefaults({});
}

/**
 * Load timeout config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged TimeoutConfig (with defaults applied).
 */
export async function loadTimeoutConfig(
  configPaths: string[],
): Promise<Required<TimeoutConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeTimeoutWithDefaults(raw as Partial<TimeoutConfig>);
    }
  }

  return mergeTimeoutWithDefaults({});
}

/**
 * Load file-checkpoint config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @param workspaceRoot - Optional resolved workspace root (forwarded to merge).
 * @returns The merged FileCheckpointConfig (with defaults applied).
 */
export async function loadFileCheckpointConfig(
  configPaths: string[],
  workspaceRoot?: string,
): Promise<Required<FileCheckpointConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeFileCheckpointConfig(raw as Partial<FileCheckpointConfig>, workspaceRoot);
    }
  }

  return mergeFileCheckpointConfig({}, workspaceRoot);
}

/**
 * Load storage config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged StorageConfig (with defaults applied).
 */
export async function loadStorageConfig(
  configPaths: string[],
): Promise<StorageConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeStorageWithDefaults(raw as Partial<StorageConfig>);
    }
  }

  return mergeStorageWithDefaults({});
}

/**
 * Load output config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged OutputConfig (with defaults applied).
 */
export async function loadOutputConfig(
  configPaths: string[],
): Promise<Required<OutputConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeOutputWithDefaults(raw as Partial<OutputConfig>);
    }
  }

  return mergeOutputWithDefaults({});
}

/**
 * Load presets config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged PresetsConfig (with defaults applied).
 */
export async function loadPresetsConfig(
  configPaths: string[],
): Promise<PresetsConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return transformPresetsConfig(raw as PresetsConfigInput);
    }
  }

  return transformPresetsConfig({});
}

/**
 * Load tool-specific configurations (readFile, writeFile, etc.).
 *
 * Reads a single config file that may contain sections for each tool,
 * routing recognizable sections through their dedicated processors.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns Loaded tool configuration object.
 */
export async function loadToolConfigs(
  configPaths: string[],
): Promise<PredefinedToolsPresetConfig["config"]> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw === null) continue;

    const rawRecord = raw as Record<string, unknown>;
    const config: PredefinedToolsPresetConfig["config"] = {};

    // Route readFile section through its dedicated processor
    if (rawRecord["readFile"] && typeof rawRecord["readFile"] === "object") {
      config["readFile"] = transformReadFileConfig(rawRecord["readFile"] as Parameters<typeof transformReadFileConfig>[0]);
    }

    // For now, pass through other tool sections without dedicated processors.
    // When writeFile/editFile/etc. processors are added, route them similarly.
    for (const key of ["writeFile", "editFile", "runShell", "sessionNote", "backendShell"] as const) {
      if (rawRecord[key] && typeof rawRecord[key] === "object") {
        (config as Record<string, unknown>)[key] = rawRecord[key];
      }
    }

    return config;
  }

  return {};
}

/**
 * Load and parse Agent Loop configuration from file.
 *
 * This function belongs in the application layer because it performs file I/O.
 * The SDK only provides pure parsing and validation functions.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Agent Loop configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadAgentLoopConfig(filePath: string): Promise<ParsedAgentLoopConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Agent Loop configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  let rawConfig: unknown;
  try {
    rawConfig = format === "toml" ? parseToml(content) : parseJson(content);
  } catch (error) {
    throw new Error(
      `Agent Loop configuration parsing failed (${format}): ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }

  const parsed: ParsedAgentLoopConfig = {
    configType: "agent_loop",
    format,
    config: rawConfig as AgentLoopConfigFile,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateAgentLoopConfig(parsed.config);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Agent Loop validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// LLM Profile Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse LLM Profile configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated LLM Profile configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadLLMProfileConfig(filePath: string): Promise<ParsedLLMProfileConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`LLM Profile configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const profile = parseLLMProfile(content, format);

  const parsed: ParsedLLMProfileConfig = {
    configType: "llm_profile",
    format,
    config: profile,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateLLMProfile(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`LLM Profile validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Node Template Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse Node Template configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Node Template configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadNodeTemplateConfig(filePath: string): Promise<ParsedNodeTemplateConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Node Template configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const template = parseNodeTemplate(content, format);

  const parsed: ParsedNodeTemplateConfig = {
    configType: "node_template",
    format,
    config: template,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateNodeTemplate(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Node Template validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Trigger Template Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse Trigger Template configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Trigger Template configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadTriggerTemplateConfig(filePath: string): Promise<ParsedTriggerTemplateConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Trigger Template configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const template = parseTriggerTemplate(content, format);

  const parsed: ParsedTriggerTemplateConfig = {
    configType: "trigger_template",
    format,
    config: template,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateTriggerTemplate(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Trigger Template validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Hook Template Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse Hook Template configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Hook Template configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadHookTemplateConfig(filePath: string): Promise<ParsedHookTemplateConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Hook Template configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const template = parseHookTemplate(content, format);

  const parsed: ParsedHookTemplateConfig = {
    configType: "hook_template",
    format,
    config: template,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateHookTemplate(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Hook Template validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Script Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse Script configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Script configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadScriptConfig(filePath: string): Promise<ParsedScriptConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Script configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const script = parseScript(content, format);

  const parsed: ParsedScriptConfig = {
    configType: "script",
    format,
    config: script,
    rawContent: content,
  };

  // Validate the configuration
  const result = validateScript(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Script validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Prompt Template Configuration Loader
// -----------------------------------------------------------------------

/**
 * Load and parse Prompt Template configuration from file.
 *
 * @param filePath - Configuration file path (TOML or JSON).
 * @returns Parsed and validated Prompt Template configuration.
 * @throws Error if file cannot be read, parsed, or validated.
 */
export async function loadPromptTemplateConfig(filePath: string): Promise<ParsedPromptTemplateConfig> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) {
    throw new Error(`Prompt Template configuration file not found: ${filePath}`);
  }

  const { content, format } = loaded;

  // Parse the content
  const template = parsePromptTemplateConfig(content, format);

  const parsed: ParsedPromptTemplateConfig = {
    configType: "prompt_template",
    format,
    config: template as PromptTemplateConfigFile,
    rawContent: content,
  };

  // Validate the configuration
  const result = validatePromptTemplate(parsed);
  if (result.isErr()) {
    const errorMessages = result.error.map((e: ValidationError) => e.message).join("\n");
    throw new Error(`Prompt Template validation failed:\n${errorMessages}`);
  }

  return parsed;
}

// -----------------------------------------------------------------------
// Infrastructure Preset Loading
// -----------------------------------------------------------------------

/**
 * Default infrastructure preset directory (configs/infrastructure in project root).
 */
export function getDefaultInfraPresetDir(projectRoot: string): string {
  return path.join(projectRoot, "configs", "infrastructure");
}

/**
 * Load an infrastructure preset definition by name.
 *
 * Resolution:
 * 1. Load `configs/infrastructure/index.json` → expand paths → discover presets
 * 2. Match `presetName` to a preset file by filename
 * 3. Load the preset file
 *
 * @param baseDir - Directory containing the preset index
 * @param presetName - Name of the preset to load
 * @returns The loaded InfrastructurePresetFile
 */
export async function loadInfrastructurePreset(
  baseDir: string,
  presetName: string,
): Promise<InfrastructurePresetFile> {
  const { resolvePresetIndex, findPresetByName, loadSingleFilePreset } = await import("./preset-loader.js");
  const resolved = await resolvePresetIndex(baseDir);
  const entry = findPresetByName(resolved.presets, presetName);

  if (!entry) {
    const available = Array.from(resolved.presets.keys()).join(", ");
    throw new Error(
      `Infrastructure preset "${presetName}" not found in ${baseDir}. Available presets: ${available || "(none)"}`,
    );
  }

  const preset = await loadSingleFilePreset<InfrastructurePresetFile>(entry);
  return preset;
}

/**
 * Resolve an infrastructure preset's file mappings into absolute paths.
 *
 * @param preset - The loaded infrastructure preset
 * @param baseDir - Base directory to resolve relative paths from
 * @returns Object with resolved absolute paths for each config domain
 */
export function resolveInfraPresetFiles(
  preset: InfrastructurePresetFile,
  baseDir: string,
): InfrastructurePresetFile["files"] & Record<string, string> {
  const resolved: Record<string, string> = {};

  for (const [key, filePath] of Object.entries(preset.files)) {
    if (filePath) {
      resolved[key] = path.resolve(baseDir, filePath);
    }
  }

  return resolved as InfrastructurePresetFile["files"] & Record<string, string>;
}

/**
 * Infrastructure config bundle — all config types loaded together.
 */
export interface InfrastructureConfigBundle {
  metrics: MetricsConfig;
  timeout: Required<TimeoutConfig>;
  fileCheckpoint: Required<FileCheckpointConfig>;
  storage: StorageConfig;
  output: Required<OutputConfig>;
  presets: PresetsConfig;
  /** Tool-specific configuration (readFile, writeFile, etc.) */
  tools: PredefinedToolsPresetConfig["config"];
  /** Sandbox global configuration (profiles, rules, etc.) */
  sandbox: SandboxGlobalConfig;
}

/**
 * Load sandbox config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged SandboxGlobalConfig (with defaults applied).
 */
export async function loadSandboxConfig(
  configPaths: string[],
): Promise<SandboxGlobalConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeSandboxWithDefaults(raw as Partial<SandboxGlobalConfig>);
    }
  }

  return mergeSandboxWithDefaults({});
}

/**
 * Load all infrastructure configs with preset support.
 *
 * Tries preset mode first (if `configs/infrastructure/index.json` exists).
 * If no preset index is found, falls back to loading from provided default paths.
 *
 * @param projectRoot - Absolute path to the project root
 * @param presetName - Name of the infrastructure preset to use (optional)
 * @param defaultPaths - Default paths for each config domain (fallback if no preset)
 * @returns Loaded InfrastructureConfigBundle
 */
export async function loadInfrastructureConfigs(
  projectRoot: string,
  presetName?: string,
  defaultPaths?: InfrastructurePresetFile["files"],
): Promise<InfrastructureConfigBundle> {
  const presetDir = getDefaultInfraPresetDir(projectRoot);
  const indexPath = path.join(presetDir, "index.json");

  // Check if preset index exists
  let fileMapping: InfrastructurePresetFile["files"] | null = null;
  try {
    await fs.access(indexPath);
  } catch {
    // No preset index — fall back to default paths
    fileMapping = defaultPaths ?? {};
  }

  // Try preset mode
  if (presetName && fileMapping === null) {
    try {
      const preset = await loadInfrastructurePreset(presetDir, presetName);
      fileMapping = preset.files;
    } catch {
      fileMapping = defaultPaths ?? {};
    }
  }

  // If still null (index exists but no preset name provided), use default paths
  if (fileMapping === null) {
    fileMapping = defaultPaths ?? {};
  }

  // Resolve file paths
  const resolved = fileMapping as InfrastructurePresetFile["files"];

  // Load each config type
  const [metrics, timeout, fileCheckpoint, storage, output, presets, tools, sandbox] =
    await Promise.all([
      resolved.metrics
        ? loadMetricsConfig([resolved.metrics])
        : loadMetricsConfig([]),
      resolved.timeout
        ? loadTimeoutConfig([resolved.timeout])
        : loadTimeoutConfig([]),
      resolved.fileCheckpoint
        ? loadFileCheckpointConfig([resolved.fileCheckpoint], projectRoot)
        : loadFileCheckpointConfig([], projectRoot),
      resolved.storage
        ? loadStorageConfig([resolved.storage])
        : loadStorageConfig([]),
      resolved.output
        ? loadOutputConfig([resolved.output])
        : loadOutputConfig([]),
      resolved.presets
        ? loadPresetsConfig([resolved.presets])
        : loadPresetsConfig([]),
      resolved.tools
        ? loadToolConfigs([resolved.tools])
        : loadToolConfigs([]),
      resolved.sandbox
        ? loadSandboxConfig([resolved.sandbox])
        : loadSandboxConfig([]),
    ]);

  return {
    metrics,
    timeout,
    fileCheckpoint,
    storage,
    output,
    presets,
    tools,
    sandbox,
  };
}

/**
 * Default infrastructure preset name used when no explicit preset is provided.
 */
export const DEFAULT_INFRA_PRESET = "development";

/**
 * Load infrastructure configs using the default preset from `configs/infrastructure/`.
 *
 * This is the recommended entry point for most applications. It loads all config
 * domains (metrics, timeout, storage, output, file-checkpoint, presets, tools)
 * from the default preset file (`development.json`) under `configs/infrastructure/`.
 *
 * Each loaded value is merged with the SDK processor's hardcoded defaults, so
 * any fields omitted in the TOML files will still have sensible default values.
 *
 * @param projectRoot - Absolute path to the project root
 * @returns Fully resolved InfrastructureConfigBundle
 *
 * @example
 * ```ts
 * import { loadDefaultInfrastructureConfigs } from "@wf-agent/config-processor";
 *
 * const { metrics, timeout, tools } = await loadDefaultInfrastructureConfigs(projectRoot);
 * // tools.readFile.maxChars → 200000 (or whatever the user set in tools.toml)
 * ```
 */
export async function loadDefaultInfrastructureConfigs(
  projectRoot: string,
): Promise<InfrastructureConfigBundle> {
  return loadInfrastructureConfigs(projectRoot, DEFAULT_INFRA_PRESET);
}