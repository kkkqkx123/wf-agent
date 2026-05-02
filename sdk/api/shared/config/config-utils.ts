/**
 * Configuration Tool Helper Functions
 * Provide auxiliary functions related to configuration files
 */

import * as path from "path";
import * as fs from "fs/promises";
import { ConfigFormat } from "./types.js";

/**
 * Detect the configuration format based on the file extension.
 * @param filePath File path
 * @returns Configuration format
 * @throws {Error} Throws an error when the file extension cannot be recognized.
 */
export function detectConfigFormat(filePath: string): ConfigFormat {
  const ext = path.extname(filePath).toLowerCase();

  switch (ext) {
    case ".toml":
      return "toml";
    case ".json":
      return "json";
    default:
      throw new Error(`Unrecognized configuration file extension: ${ext}`);
  }
}

/**
 * Load configuration file content and detect format
 * @param filePath Configuration file path
 * @returns Object containing file content and detected format
 * @throws {Error} Throws an error if file cannot be read or format is not recognized
 */
export async function loadConfigContent(
  filePath: string,
): Promise<{ content: string; format: ConfigFormat }> {
  try {
    const content = await fs.readFile(filePath, "utf-8");
    const format = detectConfigFormat(filePath);
    return { content, format };
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      throw new Error(`Configuration file not found: ${filePath}`);
    }
    throw error;
  }
}

/**
 * Load and parse Agent Loop configuration from file
 * @param filePath Configuration file path
 * @returns Parsed Agent Loop configuration
 * @throws {Error} Throws an error if file cannot be read, parsed, or validated
 */
export async function loadAgentLoopConfig(filePath: string): Promise<any> {
  // Dynamic import to avoid circular dependencies
  const { parseAndValidateAgentLoopConfig } = await import("./processors/agent-loop.js");
  
  const { content, format } = await loadConfigContent(filePath);
  return parseAndValidateAgentLoopConfig(content, format);
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
