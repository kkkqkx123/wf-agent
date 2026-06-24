/**
 * Custom Resources Loader
 *
 * Loads custom resources (tools, triggers, prompts) from JSON configuration files.
 * Supports both relative and absolute file paths.
 * Non-fatal errors are collected and returned, allowing partial loading.
 */

import { readFile } from "fs/promises";
import { resolve } from "path";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import type {
  CustomToolDefinition,
  CustomTriggerDefinition,
  CustomPromptDefinition,
  CustomResources,
  CustomResourcesPresetConfig,
} from "./types.js";

const logger = createContextualLogger({ component: "CustomResourcesLoader" });

/**
 * Attempt to load and parse a JSON file
 *
 * @param filePath Path to the JSON file (absolute or relative)
 * @param baseDir Base directory for resolving relative paths
 * @returns Parsed JSON object or null if file not found
 * @throws Error if file exists but cannot be read or parsed
 */
async function loadJsonFile(
  filePath: string,
  baseDir: string,
): Promise<Record<string, unknown> | null> {
  try {
    const resolvedPath = /^\//.test(filePath) || /^[a-zA-Z]:/.test(filePath)
      ? filePath
      : resolve(baseDir, filePath);

    const content = await readFile(resolvedPath, "utf-8");
    return JSON.parse(content);
  } catch (error) {
    if (error instanceof Error) {
      if ("code" in error && error.code === "ENOENT") {
        return null;
      }
      if (error instanceof SyntaxError) {
        throw new Error(`Failed to parse JSON file: ${filePath} - ${error.message}`);
      }
    }
    throw error;
  }
}

/**
 * Load custom tools from a JSON file
 *
 * @param filePath Path to the custom tools definition file
 * @param baseDir Base directory for resolving relative paths
 * @returns Array of custom tool definitions
 */
export async function loadCustomTools(
  filePath: string,
  baseDir: string,
): Promise<CustomToolDefinition[]> {
  try {
    const data = await loadJsonFile(filePath, baseDir);
    if (!data) {
      logger.warn(`Custom tools file not found: ${filePath}`);
      return [];
    }

    if (!Array.isArray(data["tools"])) {
      throw new Error("Expected 'tools' array in custom tools file");
    }

    const tools = data["tools"] as unknown[];
    return tools.map((tool) => validateToolDefinition(tool));
  } catch (error) {
    logger.error(`Failed to load custom tools from ${filePath}`, { error });
    throw error;
  }
}

/**
 * Load custom triggers from a JSON file
 *
 * @param filePath Path to the custom triggers definition file
 * @param baseDir Base directory for resolving relative paths
 * @returns Array of custom trigger definitions
 */
export async function loadCustomTriggers(
  filePath: string,
  baseDir: string,
): Promise<CustomTriggerDefinition[]> {
  try {
    const data = await loadJsonFile(filePath, baseDir);
    if (!data) {
      logger.warn(`Custom triggers file not found: ${filePath}`);
      return [];
    }

    if (!Array.isArray(data["triggers"])) {
      throw new Error("Expected 'triggers' array in custom triggers file");
    }

    const triggers = data["triggers"] as unknown[];
    return triggers.map((trigger) => validateTriggerDefinition(trigger));
  } catch (error) {
    logger.error(`Failed to load custom triggers from ${filePath}`, { error });
    throw error;
  }
}

/**
 * Load custom prompts from a JSON file
 *
 * @param filePath Path to the custom prompts definition file
 * @param baseDir Base directory for resolving relative paths
 * @returns Array of custom prompt definitions
 */
export async function loadCustomPrompts(
  filePath: string,
  baseDir: string,
): Promise<CustomPromptDefinition[]> {
  try {
    const data = await loadJsonFile(filePath, baseDir);
    if (!data) {
      logger.warn(`Custom prompts file not found: ${filePath}`);
      return [];
    }

    if (!Array.isArray(data["prompts"])) {
      throw new Error("Expected 'prompts' array in custom prompts file");
    }

    const prompts = data["prompts"] as unknown[];
    return prompts.map((prompt) => validatePromptDefinition(prompt));
  } catch (error) {
    logger.error(`Failed to load custom prompts from ${filePath}`, { error });
    throw error;
  }
}

/**
 * Load all custom resources from configuration
 *
 * Loads tools, triggers, and prompts from paths specified in the config.
 * Non-fatal errors are collected and returned, allowing partial loading.
 *
 * @param config Custom resources configuration
 * @param baseDir Base directory for resolving relative paths in custom resource files
 * @returns Loaded custom resources and any errors encountered during loading
 */
export async function loadCustomResourcesFromConfig(
  config: CustomResourcesPresetConfig,
  baseDir: string,
): Promise<CustomResources> {
  const errors: Array<{ path: string; error: string }> = [];
  const resources: CustomResources = {
    tools: [],
    triggers: [],
    prompts: [],
    errors,
  };

  if (!config.enabled) {
    logger.debug("Custom resources loading is disabled");
    return resources;
  }

  // Load tools
  if (config.toolsPath) {
    try {
      resources.tools = await loadCustomTools(config.toolsPath, baseDir);
      logger.info(`Loaded ${resources.tools.length} custom tools from ${config.toolsPath}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push({ path: config.toolsPath, error: message });
      logger.warn(`Failed to load custom tools: ${message}`);
    }
  }

  // Load triggers
  if (config.triggersPath) {
    try {
      resources.triggers = await loadCustomTriggers(config.triggersPath, baseDir);
      logger.info(
        `Loaded ${resources.triggers.length} custom triggers from ${config.triggersPath}`,
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push({ path: config.triggersPath, error: message });
      logger.warn(`Failed to load custom triggers: ${message}`);
    }
  }

  // Load prompts
  if (config.promptsPath) {
    try {
      resources.prompts = await loadCustomPrompts(config.promptsPath, baseDir);
      logger.info(`Loaded ${resources.prompts.length} custom prompts from ${config.promptsPath}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push({ path: config.promptsPath, error: message });
      logger.warn(`Failed to load custom prompts: ${message}`);
    }
  }

  return resources;
}

/**
 * Validate and cast a tool definition
 */
function validateToolDefinition(obj: unknown): CustomToolDefinition {
  if (typeof obj !== "object" || !obj) {
    throw new Error("Invalid tool definition: not an object");
  }

  const tool = obj as Record<string, unknown>;

  if (typeof tool["id"] !== "string") {
    throw new Error("Invalid tool definition: missing or invalid 'id' field");
  }
  if (typeof tool["type"] !== "string") {
    throw new Error(`Invalid tool '${tool["id"]}': missing or invalid 'type' field`);
  }
  if (typeof tool["description"] !== "object" || !tool["description"]) {
    throw new Error(`Invalid tool '${tool["id"]}': missing or invalid 'description' field`);
  }
  if (typeof tool["schema"] !== "object" || !tool["schema"]) {
    throw new Error(`Invalid tool '${tool["id"]}': missing or invalid 'schema' field`);
  }
  if (typeof tool["handler"] !== "object" || !tool["handler"]) {
    throw new Error(`Invalid tool '${tool["id"]}': missing or invalid 'handler' field`);
  }

  return tool as unknown as CustomToolDefinition;
}

/**
 * Validate and cast a trigger definition
 */
function validateTriggerDefinition(obj: unknown): CustomTriggerDefinition {
  if (typeof obj !== "object" || !obj) {
    throw new Error("Invalid trigger definition: not an object");
  }

  const trigger = obj as Record<string, unknown>;

  if (typeof trigger["name"] !== "string") {
    throw new Error("Invalid trigger definition: missing or invalid 'name' field");
  }
  if (typeof trigger["description"] !== "string") {
    throw new Error(
      `Invalid trigger '${trigger["name"]}': missing or invalid 'description' field`,
    );
  }
  if (typeof trigger["condition"] !== "object" || !trigger["condition"]) {
    throw new Error(
      `Invalid trigger '${trigger["name"]}': missing or invalid 'condition' field`,
    );
  }

  return trigger as unknown as CustomTriggerDefinition;
}

/**
 * Validate and cast a prompt definition
 */
function validatePromptDefinition(obj: unknown): CustomPromptDefinition {
  if (typeof obj !== "object" || !obj) {
    throw new Error("Invalid prompt definition: not an object");
  }

  const prompt = obj as Record<string, unknown>;

  if (typeof prompt["id"] !== "string") {
    throw new Error("Invalid prompt definition: missing or invalid 'id' field");
  }
  if (typeof prompt["name"] !== "string") {
    throw new Error(`Invalid prompt '${prompt["id"]}': missing or invalid 'name' field`);
  }
  if (typeof prompt["content"] !== "string") {
    throw new Error(`Invalid prompt '${prompt["id"]}': missing or invalid 'content' field`);
  }
  if (typeof prompt["type"] !== "string") {
    throw new Error(`Invalid prompt '${prompt["id"]}': missing or invalid 'type' field`);
  }

  return prompt as unknown as CustomPromptDefinition;
}
