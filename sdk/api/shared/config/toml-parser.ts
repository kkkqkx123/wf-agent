/**
 * TOML Parser
 * Responsible for parsing configuration files in TOML format
 *
 * Design Principles:
 * - Use TomlParserManager to obtain the TOML parser
 * - Provide clear error messages
 * - Implement unified error handling
 * - Maintain a consistent interface to avoid impacting existing code
 */

import type { WorkflowConfigFile } from "./types.js";
import { ConfigurationError } from "@wf-agent/types";
import { TomlParserManager } from "../../../utils/toml-parser-manager.js";
import { isError } from "@wf-agent/common-utils";

/**
 * Get a TOML parser instance
 * @returns TOML parser
 * @throws {ConfigurationError} Throws when the TOML parsing library is not found
 */
function getTomlParser(): { parse: (content: string) => unknown } {
  return TomlParserManager.getInstance() as { parse: (content: string) => unknown };
}

/**
 * Remove Symbol keys from an object recursively.
 * The @iarna/toml parser adds Symbol metadata (Symbol(type), Symbol(declared))
 * to parsed objects. These Symbol keys cause issues with Zod's record validation
 * that expects all keys to be strings.
 *
 * Uses Object.getOwnPropertyNames() to get only string keys,
 * which is the standard JavaScript pattern for excluding Symbol properties.
 * This approach is faster and more semantic than JSON serialization.
 *
 * @param obj The object to clean
 * @returns A new object without Symbol keys
 */
function removeSymbolKeys<T>(obj: T): T {
  if (obj === null || typeof obj !== "object") {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(item => removeSymbolKeys(item)) as T;
  }

  // Only get string keys, automatically excluding Symbols
  const stringKeys = Object.getOwnPropertyNames(obj);
  const result: Record<string, unknown> = {};

  for (const key of stringKeys) {
    result[key] = removeSymbolKeys((obj as Record<string, unknown>)[key]);
  }

  return result as T;
}

/**
 * Parse TOML content
 * @param content A string containing the TOML content
 * @returns A parsed configuration object
 * @throws {ConfigurationError} Throws this exception if the TOML parsing fails or the format is incorrect
 */
export function parseToml(content: string): WorkflowConfigFile {
  try {
    const toml = getTomlParser();
    const rawParsed = toml.parse(content);

    // Remove Symbol metadata keys added by @iarna/toml parser
    // These cause issues with Zod's record validation
    const parsed = removeSymbolKeys(rawParsed);

    // Verify the parsing results.
    const parsedRecord = parsed as Record<string, unknown>;
    if (!parsedRecord["workflow"]) {
      throw new ConfigurationError(
        "The TOML configuration file must contain a [workflow] section",
        "workflow",
      );
    }

    return parsed as unknown as WorkflowConfigFile;
  } catch (error) {
    if (error instanceof ConfigurationError) {
      throw error;
    }
    if (isError(error)) {
      throw new ConfigurationError(`TOML parsing failed: ${error.message}`, undefined, {
        originalError: error.message,
      });
    }
    throw new ConfigurationError("TOML parsing failure: unknown error");
  }
}

/**
 * Verify the basic format of the TOML content
 * @param content A string containing the TOML content
 * @returns Whether it is valid
 */
export function validateTomlSyntax(content: string): boolean {
  try {
    parseToml(content);
    return true;
  } catch {
    return false;
  }
}
