/**
 * Agent Loop Configuration Processor
 *
 * Handles the parsing and transformation of AgentLoopConfigFile to AgentLoopConfig
 */

import type {
  AgentLoopConfig,
  AgentHook,
  AgentHookType,
} from "@wf-agent/types";
import type { Condition } from "@wf-agent/types";
import type {
  AgentLoopConfigFile,
  AgentHookConfigFile,
  ParsedAgentLoopConfig,
  ConfigFormat,
} from "../types.js";
import { parseToml } from "../toml-parser.js";
import { parseJson } from "../json-parser.js";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import {
  validateAgentLoopConfig,
  getAgentLoopValidationWarnings,
} from "../validators/agent-loop-validator.js";
import { stringifyJson } from "../json-parser.js";

/**
 * Basic structure check: Verify whether the parsed result has the basic structure of AgentLoopConfigFile
 * @param rawConfig The original parsed result
 * @returns Whether the basic check was passed
 */
function hasBasicConfigStructure(rawConfig: unknown): rawConfig is AgentLoopConfigFile {
  if (!rawConfig || typeof rawConfig !== "object") {
    return false;
  }
  const config = rawConfig as Record<string, unknown>;
  // Check the required field 'id'.
  if (!("id" in config)) {
    return false;
  }
  return true;
}

/**
 * Parse the Agent Loop configuration file (only parsing, no validation)
 * @param content The content of the configuration file
 * @param format The format of the configuration
 * @returns The parsed configuration
 * @throws Throws an error if the parsing fails or the basic structure does not match
 */
export function parseAgentLoopConfig(content: string, format: ConfigFormat): ParsedAgentLoopConfig {
  let rawConfig: unknown;

  // Parse configuration content
  try {
    rawConfig = format === "toml" ? parseToml(content) : parseJson(content);
  } catch (error) {
    throw new Error(
      `Configuration parsing failed (${format}): ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }

  // Basic structure check
  if (!hasBasicConfigStructure(rawConfig)) {
    throw new Error(
      'Invalid configuration structure: required field "id" is missing or configuration is not a valid object',
    );
  }

  return {
    configType: "agent_loop",
    format,
    config: rawConfig,
    rawContent: content,
  };
}

/**
 * Parse and validate the Agent Loop configuration
 * @param content: The content of the configuration file
 * @param format: The format of the configuration
 * @param onWarning: An optional warning callback function, used to receive validation warnings
 * @returns: The validated configuration
 * @throws: Throws an error when parsing fails, the basic structure does not match, or the validation fails
 */
export function parseAndValidateAgentLoopConfig(
  content: string,
  format: ConfigFormat,
  onWarning?: (warnings: string[]) => void,
): ParsedAgentLoopConfig {
  const parsed = parseAgentLoopConfig(content, format);
  const result = validateAgentLoopConfig(parsed.config);

  if (result.isErr()) {
    const errorMessages = result.error.map((err: ValidationError) => err.message).join("\n");
    throw new Error(`Agent Loop validation failed:\n${errorMessages}`);
  }

  // Check and report warnings (via callback functions, not console.warn)
  const warnings = getAgentLoopValidationWarnings(parsed.config);
  if (warnings.length > 0 && onWarning) {
    onWarning(warnings);
  }

  return parsed;
}

/**
 * Convert the configuration file format to runtime configuration
 * @param configFile The agent loop configuration file
 * @returns AgentLoopConfig
 */
export function transformToAgentLoopConfig(configFile: AgentLoopConfigFile): AgentLoopConfig {
  const config: AgentLoopConfig = {
    profileId: configFile.profileId,
    systemPrompt: configFile.systemPrompt,
    maxIterations: configFile.maxIterations,
    initialMessages: configFile.initialMessages,
    tools: configFile.tools,
    stream: configFile.stream,
    createCheckpointOnEnd: configFile.checkpoint?.createOnEnd,
    createCheckpointOnError: configFile.checkpoint?.createOnError,
  };

  // Translate hooks
  if (configFile.hooks && configFile.hooks.length > 0) {
    config.hooks = configFile.hooks.map(hook => transformHook(hook));
  }

  return config;
}

/**
 * Export Agent Loop configuration
 * @param configFile AgentLoopConfigFile object
 * @param format Configuration format
 * @returns String containing the configuration file content
 */
export function exportAgentLoopConfig(configFile: AgentLoopConfigFile, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(configFile, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support export, please use JSON format",
        format,
        {
          suggestion: "Use JSON instead",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}

/**
 * Translate Hook Configuration
 * @param hookFile The Hook configuration file
 * @returns AgentHook
 */
function transformHook(hookFile: AgentHookConfigFile): AgentHook {
  const hook: AgentHook = {
    hookType: hookFile.hookType as AgentHookType,
    eventName: hookFile.eventName,
    enabled: hookFile.enabled,
    weight: hookFile.weight,
    eventPayload: hookFile.eventPayload,
    createCheckpoint: hookFile.createCheckpoint,
    checkpointDescription: hookFile.checkpointDescription,
  };

  // Parse conditional expressions
  if (hookFile.condition) {
    hook.condition = parseCondition(hookFile.condition);
  }

  return hook;
}

/**
 * Parse a conditional expression
 * @param conditionStr: The condition string
 * @returns: A Condition object
 */
function parseCondition(conditionStr: string): Condition {
  // Returns expression conditions directly
  // The Condition interface only requires the expression field, and metadata is optional
  return {
    expression: conditionStr,
  };
}
