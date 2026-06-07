/**
 * Configuration Utility Functions
 * 
 * Provides utility functions for configuration processing.
 * Focus on parameter substitution and transformation logic.
 */

import * as fs from "fs/promises";
import type { ParsedAgentLoopConfig, AgentLoopConfigFile } from "../types.js";
import { getConfigFormatFromPath } from "../parsers/format-detector.js";
import { parseJson } from "../parsers/json-parser.js";
import { parseToml } from "../parsers/toml-parser.js";
import { validateAgentLoopConfig } from "../../../../agent/validation/agent-loop-validator.js";

/**
 * Load and parse Agent Loop configuration from file
 * @param filePath Configuration file path
 * @returns Parsed Agent Loop configuration
 * @throws {Error} Throws an error if file cannot be read, parsed, or validated
 */
export async function loadAgentLoopConfig(filePath: string): Promise<ParsedAgentLoopConfig> {
  const content = await fs.readFile(filePath, "utf-8");
  const format = getConfigFormatFromPath(filePath);

  // Parse the content
  let rawConfig: unknown;
  try {
    rawConfig = format === "toml" ? parseToml(content) : parseJson(content);
  } catch (error) {
    throw new Error(
      `Configuration parsing failed (${format}): ${error instanceof Error ? error.message : String(error)}`,
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
    const errorMessages = result.error.map((e) => e.message).join("\n");
    throw new Error(`Agent Loop validation failed:\n${errorMessages}`);
  }

  return parsed;
}

/**
 * Substitute parameters in an object by replacing {{parameters.xxx}} placeholders
 * @param obj The object to process
 * @param parameters The parameter object containing replacement values
 * @returns A new object with parameters substituted
 */
export function substituteParameters<T>(
  obj: T,
  parameters: Record<string, unknown>,
): T {
  // If no parameters, return original object
  if (!parameters || Object.keys(parameters).length === 0) {
    return obj;
  }

  // Deep clone and replace parameters
  const processed = JSON.parse(JSON.stringify(obj));
  replaceParametersInObject(processed, parameters);
  return processed;
}

/**
 * Recursively replace parameter placeholders in an object
 * @param obj The object to be processed (modified in place)
 * @param parameters The parameter object
 */
function replaceParametersInObject(
  obj: unknown,
  parameters: Record<string, unknown>,
): void {
  if (Array.isArray(obj)) {
    for (let i = 0; i < obj.length; i++) {
      if (typeof obj[i] === 'string') {
        obj[i] = replaceParameterInString(obj[i] as string, parameters);
      } else if (typeof obj[i] === 'object' && obj[i] !== null) {
        replaceParametersInObject(obj[i], parameters);
      }
    }
  } else if (obj && typeof obj === 'object') {
    const objRecord = obj as Record<string, unknown>;
    for (const key in objRecord) {
      if (Object.prototype.hasOwnProperty.call(objRecord, key)) {
        if (typeof objRecord[key] === 'string') {
          objRecord[key] = replaceParameterInString(
            objRecord[key] as string,
            parameters,
          );
        } else if (typeof objRecord[key] === 'object' && objRecord[key] !== null) {
          replaceParametersInObject(objRecord[key], parameters);
        }
      }
    }
  }
}

/**
 * Replace parameter placeholders in a string
 * Matches {{parameters.paramName}} pattern and replaces with actual value
 * @param str The string to process
 * @param parameters The parameter object
 * @returns The string with placeholders replaced
 */
function replaceParameterInString(
  str: string,
  parameters: Record<string, unknown>,
): string {
  const regex = /\{\{parameters\.(\w+)\}\}/g;
  return str.replace(regex, (match, paramName: string) => {
    if (parameters[paramName] !== undefined) {
      return String(parameters[paramName]);
    }
    return match; // Keep original if parameter not found
  });
}
